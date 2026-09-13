use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Read;
use std::process::Stdio;
use std::thread;
use std::time::{Duration, Instant};

use gpui::Point;

use crate::theme::ActiveTheme as _;
use crate::ui::scrollbar;
use crate::ui::shimmer::{ShimmerStyle, ShimmerText};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PullRequestDetailTab {
    Summary,
    Commits,
    Code,
}

impl PullRequestDetailTab {
    const ALL: [Self; 3] = [Self::Summary, Self::Commits, Self::Code];

    fn label(self) -> &'static str {
        match self {
            Self::Summary => "Summary",
            Self::Commits => "Commits",
            Self::Code => "Changes",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct PullRequestCommit {
    oid: String,
    message: String,
    authored_at: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct PullRequestCheck {
    name: String,
    state: String,
    bucket: String,
    #[serde(default)]
    link: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct PullRequestComment {
    author: String,
    body: String,
    created_at: String,
    #[serde(default)]
    location: Option<String>,
}

#[derive(Deserialize)]
struct GhPullRequestCommit {
    oid: String,
    #[serde(rename = "messageHeadline")]
    message_headline: String,
    #[serde(rename = "authoredDate")]
    authored_date: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PullRequestTab {
    All,
    Open,
    Closed,
    Merged,
}

impl PullRequestTab {
    const ALL: [Self; 4] = [Self::All, Self::Open, Self::Closed, Self::Merged];

    fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Open => "Open",
            Self::Closed => "Closed",
            Self::Merged => "Merged",
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct PullRequest {
    number: u64,
    title: String,
    #[serde(default)]
    author: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    body_loaded: bool,
    repository: String,
    state: String,
    #[serde(default)]
    url: String,
    updated_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    cached_commits: Vec<PullRequestCommit>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    cached_diff: Option<String>,
}

#[derive(Deserialize)]
struct GhPullRequest {
    number: u64,
    title: String,
    #[serde(default)]
    author: GhAuthor,
    #[serde(default)]
    body: Option<String>,
    state: String,
    #[serde(default)]
    url: String,
    #[serde(rename = "mergedAt", default)]
    merged_at: Option<String>,
    #[serde(rename = "updatedAt")]
    updated_at: Option<String>,
    #[serde(default)]
    repository: GhRepository,
}

#[derive(Default, Deserialize)]
struct GhAuthor {
    #[serde(default)]
    login: String,
}

#[derive(Deserialize, Default)]
struct GhRepository {
    #[serde(rename = "nameWithOwner")]
    name_with_owner: String,
}

const GH_FIELDS: &str = "number,title,author,body,state,updatedAt,url,repository";

fn load_owned_repository_pull_requests() -> anyhow::Result<Vec<GhPullRequest>> {
    let login = std::process::Command::new("gh")
        .args(["api", "user", "--jq", ".login"])
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "cat")
        .stdin(Stdio::null())
        .output()?;
    if !login.status.success() {
        anyhow::bail!("gh api user failed");
    }
    let owner = String::from_utf8_lossy(&login.stdout).trim().to_owned();
    let output = std::process::Command::new("gh")
        .args([
            "search", "prs", "--owner", &owner, "--state", "open", "--limit", "1000", "--json",
            GH_FIELDS,
        ])
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "cat")
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "gh search prs --owner failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let mut requests: Vec<GhPullRequest> = serde_json::from_slice(&output.stdout)?;
    let closed = std::process::Command::new("gh")
        .args([
            "search", "prs", "--owner", &owner, "--state", "closed", "--limit", "1000", "--json",
            GH_FIELDS,
        ])
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "cat")
        .stdin(Stdio::null())
        .output()?;
    if closed.status.success() {
        requests.extend(serde_json::from_slice::<Vec<GhPullRequest>>(
            &closed.stdout,
        )?);
    }
    Ok(requests)
}

fn gh_output(args: &[&str]) -> anyhow::Result<std::process::Output> {
    let mut command = std::process::Command::new("gh");
    command
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "cat")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().expect("stdout was piped");
    let stderr = child.stderr.take().expect("stderr was piped");
    let stdout_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.take(16 * 1024 * 1024).read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr_reader = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.take(16 * 1024 * 1024).read_to_end(&mut bytes).map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(45);
    let status = loop {
        match child.try_wait()? {
            Some(status) => break status,
            None if Instant::now() < deadline => thread::sleep(Duration::from_millis(25)),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                anyhow::bail!("GitHub request timed out after 45 seconds");
            }
        }
    };
    Ok(std::process::Output {
        status,
        stdout: stdout_reader.join().map_err(|_| anyhow::anyhow!("stdout reader failed"))??,
        stderr: stderr_reader.join().map_err(|_| anyhow::anyhow!("stderr reader failed"))??,
    })
}

fn pull_request_cache_path() -> std::path::PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Insulator")
        .join("pull-requests.json")
}

pub(super) fn load_cached_pull_requests() -> Vec<PullRequest> {
    let mut entries: Vec<PullRequest> = std::fs::read(pull_request_cache_path())
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();
    // Older cache files predate `body_loaded`; a non-empty body is already a
    // successful load and must not trigger another GitHub request on startup.
    for entry in &mut entries {
        entry.body_loaded |= !entry.body.is_empty();
    }
    entries
}

pub(super) fn cached_commits(
    entries: &[PullRequest],
) -> HashMap<(String, u64), Vec<PullRequestCommit>> {
    entries
        .iter()
        .filter(|entry| !entry.cached_commits.is_empty())
        .map(|entry| {
            (
                (entry.repository.clone(), entry.number),
                entry.cached_commits.clone(),
            )
        })
        .collect()
}

pub(super) fn cached_diffs(
    entries: &[PullRequest],
) -> HashMap<(String, u64), Arc<ReviewDiffSnapshot>> {
    entries
        .iter()
        .filter_map(|entry| {
            entry.cached_diff.as_ref().map(|patch| {
                let snapshot = crate::review_diff::parse_collected(
                    ReviewDiffSource::Committed,
                    "",
                    patch,
                    true,
                );
                if snapshot.files.is_empty() {
                    None
                } else {
                    Some(((entry.repository.clone(), entry.number), Arc::new(snapshot)))
                }
            })
        })
        .flatten()
        .collect()
}

fn preserve_cached_details(entries: &mut [PullRequest], cached: &[PullRequest]) {
    let cached = cached
        .iter()
        .map(|entry| ((entry.repository.as_str(), entry.number), entry))
        .collect::<HashMap<_, _>>();
    for entry in entries {
        let Some(previous) = cached.get(&(entry.repository.as_str(), entry.number)) else {
            continue;
        };
        if !entry.body_loaded {
            entry.body.clone_from(&previous.body);
            entry.body_loaded = previous.body_loaded;
        }
        if entry.url.is_empty() {
            entry.url.clone_from(&previous.url);
        }
        entry.cached_commits.clone_from(&previous.cached_commits);
        entry.cached_diff.clone_from(&previous.cached_diff);
    }
}

fn save_cached_pull_requests(entries: &[PullRequest]) {
    let path = pull_request_cache_path();
    let Some(parent) = path.parent() else { return };
    let _ = std::fs::create_dir_all(parent);
    let Ok(bytes) = serde_json::to_vec(entries) else {
        return;
    };
    let _ = std::fs::write(path, bytes);
}

fn load_pull_request_body(request: &PullRequest) -> anyhow::Result<(String, String)> {
    #[derive(Deserialize)]
    struct Response {
        #[serde(default)]
        body: String,
        #[serde(default)]
        url: String,
    }
    let output = std::process::Command::new("gh")
        .args([
            "pr",
            "view",
            &request.number.to_string(),
            "--repo",
            &request.repository,
            "--json",
            "body,url",
        ])
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "cat")
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "gh pr view failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let response: Response = serde_json::from_slice(&output.stdout)?;
    Ok((response.body, response.url))
}

fn load_pull_request_diff(request: &PullRequest) -> anyhow::Result<(ReviewDiffSnapshot, String)> {
    #[derive(Deserialize)]
    struct File {
        filename: String,
        additions: u64,
        deletions: u64,
        #[serde(default)]
        patch: Option<String>,
    }
    let endpoint = format!(
        "repos/{}/pulls/{}/files",
        request.repository, request.number
    );
    let output = std::process::Command::new("gh")
        .args(["api", "--paginate", "--slurp", &endpoint])
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "cat")
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "gh api pull-request files failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let pages: Vec<Vec<File>> = serde_json::from_slice(&output.stdout)?;
    let mut numstat = String::new();
    let mut patch = String::new();
    for file in pages.into_iter().flatten() {
        numstat.push_str(&format!(
            "{}\t{}\t{}\n",
            file.additions, file.deletions, file.filename
        ));
        if let Some(body) = file.patch {
            patch.push_str(&format!(
                "diff --git a/{0} b/{0}\n--- a/{0}\n+++ b/{0}\n{1}\n",
                file.filename, body
            ));
        }
    }
    let snapshot =
        crate::review_diff::parse_collected(ReviewDiffSource::Committed, &numstat, &patch, true);
    if !snapshot.files.is_empty() {
        return Ok((snapshot, patch));
    }

    let fallback = std::process::Command::new("gh")
        .args([
            "pr",
            "diff",
            &request.number.to_string(),
            "--repo",
            &request.repository,
        ])
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "cat")
        .stdin(Stdio::null())
        .output()?;
    if !fallback.status.success() {
        anyhow::bail!("GitHub returned no readable pull-request patch");
    }
    let fallback_patch = String::from_utf8_lossy(&fallback.stdout).into_owned();
    Ok((
        crate::review_diff::parse_collected(ReviewDiffSource::Committed, "", &fallback_patch, true),
        fallback_patch,
    ))
}

fn without_html_comments(body: &str) -> String {
    let mut result = String::with_capacity(body.len());
    let mut rest = body;
    while let Some(start) = rest.find("<!--") {
        result.push_str(&rest[..start]);
        let Some(end) = rest[start + 4..].find("-->") else {
            break;
        };
        rest = &rest[start + 4 + end + 3..];
    }
    result.push_str(rest);
    let mut plain = String::with_capacity(result.len());
    let mut rest = result.as_str();
    while let Some(start) = rest.find('<') {
        plain.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            plain.push_str(&rest[start..]);
            break;
        };
        rest = &rest[start + end + 1..];
    }
    if !rest.is_empty() {
        plain.push_str(rest);
    }
    plain.replace("&nbsp;", " ").trim().to_owned()
}

fn load_pull_request_checks(request: &PullRequest) -> anyhow::Result<Vec<PullRequestCheck>> {
    let output = gh_output(&[
        "pr",
        "checks",
        &request.number.to_string(),
        "--repo",
        &request.repository,
        "--json",
        "name,state,bucket,link",
    ])?;
    if let Ok(checks) = serde_json::from_slice(&output.stdout) {
        return Ok(checks);
    }
    anyhow::bail!(
        "gh pr checks failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    )
}

fn load_pull_request_comments(request: &PullRequest) -> anyhow::Result<Vec<PullRequestComment>> {
    #[derive(Deserialize)]
    struct Response {
        body: String,
        created_at: String,
        user: Option<GhCommentUser>,
        path: Option<String>,
        line: Option<u64>,
    }
    #[derive(Deserialize)]
    struct GhCommentUser {
        login: String,
    }
    fn load_endpoint(endpoint: &str) -> anyhow::Result<Vec<Response>> {
        let output = gh_output(&["api", "--paginate", "--slurp", endpoint])?;
        if !output.status.success() {
            anyhow::bail!(
                "GitHub comments lookup failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        let pages: Vec<Vec<Response>> = serde_json::from_slice(&output.stdout)?;
        Ok(pages.into_iter().flatten().collect())
    }

    let issue_endpoint = format!("repos/{}/issues/{}/comments", request.repository, request.number);
    let review_endpoint = format!("repos/{}/pulls/{}/comments", request.repository, request.number);
    let mut comments = load_endpoint(&issue_endpoint)?;
    comments.extend(load_endpoint(&review_endpoint)?);
    comments.sort_by(|left, right| left.created_at.cmp(&right.created_at));
    Ok(comments
        .into_iter()
        .map(|comment| PullRequestComment {
            author: comment.user.map(|user| user.login).unwrap_or_else(|| "unknown".into()),
            body: comment.body,
            created_at: comment.created_at,
            location: comment.path.map(|path| match comment.line {
                Some(line) => format!("{path}:{line}"),
                None => path,
            }),
        })
        .collect())
}

fn post_pull_request_comment(request: &PullRequest, body: &str) -> anyhow::Result<()> {
    let output = gh_output(&[
        "pr",
        "comment",
        &request.number.to_string(),
        "--repo",
        &request.repository,
        "--body",
        body,
    ])?;
    if !output.status.success() {
        anyhow::bail!(
            "posting pull-request comment failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(())
}

fn load_pull_request_commits(request: &PullRequest) -> anyhow::Result<Vec<PullRequestCommit>> {
    let output = std::process::Command::new("gh")
        .args([
            "pr",
            "view",
            &request.number.to_string(),
            "--repo",
            &request.repository,
            "--json",
            "commits",
        ])
        .env("GH_PROMPT_DISABLED", "1")
        .env("GH_PAGER", "cat")
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        anyhow::bail!(
            "gh pr view failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    #[derive(Deserialize)]
    struct Response {
        commits: Vec<GhPullRequestCommit>,
    }
    let response: Response = serde_json::from_slice(&output.stdout)?;
    Ok(response
        .commits
        .into_iter()
        .map(|commit| PullRequestCommit {
            oid: commit.oid,
            message: commit.message_headline,
            authored_at: commit.authored_date.unwrap_or_default(),
        })
        .collect())
}

fn load_pull_requests() -> anyhow::Result<Vec<PullRequest>> {
    fn search(args: &[&str], state: Option<&str>) -> anyhow::Result<Vec<GhPullRequest>> {
        let mut command = std::process::Command::new("gh");
        command
            .args(["search", "prs"])
            .args(args)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_PAGER", "cat")
            .stdin(Stdio::null());
        if let Some(state) = state {
            command.args(["--state", state]);
        }
        let output = command
            .args(["--limit", "1000", "--json", GH_FIELDS])
            .output()?;
        if !output.status.success() {
            anyhow::bail!(
                "gh search prs failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }

    let authored_open = search(&["--author", "@me"], Some("open"))?;
    let authored_closed = search(&["--author", "@me"], Some("closed"))?;
    let owned_repository_requests = load_owned_repository_pull_requests()?;
    let mut entries: HashMap<(String, u64), PullRequest> = HashMap::new();
    for requests in [authored_open, authored_closed, owned_repository_requests] {
        for request in requests {
            let key = (request.repository.name_with_owner.clone(), request.number);
            let state =
                if request.merged_at.is_some() || request.state.eq_ignore_ascii_case("merged") {
                    "merged".to_owned()
                } else {
                    request.state.to_ascii_lowercase()
                };
            entries.entry(key).or_insert_with(|| PullRequest {
                number: request.number,
                title: request.title.clone(),
                author: request.author.login.clone(),
                body: request.body.clone().unwrap_or_default(),
                body_loaded: request.body.is_some(),
                repository: request.repository.name_with_owner.clone(),
                state,
                url: request.url.clone(),
                updated_at: request.updated_at.clone().unwrap_or_default(),
                cached_commits: Vec::new(),
                cached_diff: None,
            });
        }
    }
    let mut entries = entries.into_values().collect::<Vec<_>>();
    entries.sort_by(|left, right| right.updated_at.cmp(&left.updated_at));
    Ok(entries)
}

impl Insulator {
    pub(super) fn ensure_pull_requests(&mut self, force: bool, cx: &mut Context<Self>) {
        if self.pull_requests_refreshing {
            return;
        }
        self.pull_requests_refreshing = true;
        self.pull_requests_loading = self.pull_requests.is_empty() || force;
        let entity = cx.entity().downgrade();
        let cached = self.pull_requests.clone();
        cx.spawn(async move |_, cx| {
            let lookup = cx.background_executor().spawn(async move {
                let mut entries = load_pull_requests()?;
                preserve_cached_details(&mut entries, &cached);
                save_cached_pull_requests(&entries);
                Ok::<_, anyhow::Error>(entries)
            });
            let result = futures_lite::future::race(lookup, async {
                smol::Timer::after(Duration::from_secs(45)).await;
                Err(anyhow::anyhow!(
                    "GitHub pull-request lookup timed out after 45 seconds"
                ))
            })
            .await;
            let _ = entity.update(cx, |this, cx| {
                this.pull_requests_loading = false;
                this.pull_requests_refreshing = false;
                match result {
                    Ok(entries) => {
                        this.pull_requests = entries;
                        this.pull_requests_error = None;
                    }
                    Err(error) => this.pull_requests_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn ensure_pull_request_body(
        &mut self,
        request: PullRequest,
        cx: &mut Context<Self>,
    ) {
        let request_key = (request.repository.clone(), request.number);
        if request.body_loaded || self.pull_request_detail_loading.contains(&request_key) {
            return;
        }
        self.pull_request_detail_loading.insert(request_key.clone());
        let entity = cx.entity().downgrade();
        let key = request_key.clone();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load_pull_request_body(&request) })
                .await;
            let _ = entity.update(cx, |this, cx| {
                this.pull_request_detail_loading.remove(&key);
                match result {
                    Ok((body, url)) => {
                    this.pull_request_body_error.remove(&key);
                    if let Some(entry) = this.pull_requests.iter_mut().find(|entry| {
                        entry.repository == key.0 && entry.number == key.1
                    }) {
                        entry.body = body.clone();
                        entry.body_loaded = true;
                        entry.url = url.clone();
                        save_cached_pull_requests(&this.pull_requests);
                    }
                    if let Some(detail) = this.pull_request_detail.as_mut()
                        && detail.repository == key.0
                        && detail.number == key.1
                    {
                        detail.body = body;
                        detail.body_loaded = true;
                        detail.url = url;
                    }
                    }
                    Err(error) => {
                        this.pull_request_body_error.insert(key, error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn ensure_pull_request_checks(
        &mut self,
        request: PullRequest,
        cx: &mut Context<Self>,
    ) {
        let key = (request.repository.clone(), request.number);
        if self.pull_request_checks.contains_key(&key)
            || self.pull_request_checks_loading.contains(&key)
        {
            return;
        }
        self.pull_request_checks_loading.insert(key.clone());
        let entity = cx.entity().downgrade();
        let request_key = key.clone();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load_pull_request_checks(&request) })
                .await;
            let _ = entity.update(cx, |this, cx| {
                this.pull_request_checks_loading.remove(&request_key);
                match result {
                    Ok(checks) => {
                        this.pull_request_checks.insert(request_key.clone(), checks);
                        this.pull_request_checks_error.remove(&request_key);
                    }
                    Err(error) => {
                        this.pull_request_checks_error.insert(request_key, error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn ensure_pull_request_comments(
        &mut self,
        request: PullRequest,
        cx: &mut Context<Self>,
    ) {
        let key = (request.repository.clone(), request.number);
        if self.pull_request_comments.contains_key(&key)
            || self.pull_request_comments_loading.contains(&key)
        {
            return;
        }
        self.pull_request_comments_loading.insert(key.clone());
        let entity = cx.entity().downgrade();
        let request_key = key.clone();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load_pull_request_comments(&request) })
                .await;
            let _ = entity.update(cx, |this, cx| {
                this.pull_request_comments_loading.remove(&request_key);
                match result {
                    Ok(comments) => {
                        this.pull_request_comments.insert(request_key.clone(), comments);
                        this.pull_request_comments_error.remove(&request_key);
                    }
                    Err(error) => {
                        this.pull_request_comments_error.insert(request_key, error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn submit_pull_request_comment(
        &mut self,
        request: PullRequest,
        cx: &mut Context<Self>,
    ) {
        let body = self.pull_request_comment_input.read(cx).content().trim().to_owned();
        let request_key = (request.repository.clone(), request.number);
        if body.is_empty()
            || self.pull_request_comment_posting
            || self.pull_request_comments_loading.contains(&request_key)
        {
            return;
        }
        self.pull_request_comment_posting = true;
        let submitted_body = body.clone();
        let submitted_key = request_key.clone();
        let entity = cx.entity().downgrade();
        let request_for_load = request.clone();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { post_pull_request_comment(&request_for_load, &body) })
                .await;
            let _ = entity.update(cx, |this, cx| {
                this.pull_request_comment_posting = false;
                if result.is_ok() {
                    let still_current_pr = this
                        .pull_request_detail
                        .as_ref()
                        .is_some_and(|detail| (detail.repository.clone(), detail.number) == submitted_key);
                    let still_submitted = this.pull_request_comment_input.read(cx).content().trim() == submitted_body;
                    if still_current_pr && still_submitted {
                        this.pull_request_comment_input.update(cx, |input, cx| input.clear(cx));
                    }
                    this.pull_request_comments.remove(&(request.repository.clone(), request.number));
                    this.ensure_pull_request_comments(request.clone(), cx);
                } else if let Err(error) = result {
                    this.pull_request_comments_error.insert(
                        (request.repository.clone(), request.number),
                        error.to_string(),
                    );
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn ensure_pull_request_diff(
        &mut self,
        request: PullRequest,
        cx: &mut Context<Self>,
    ) {
        let key = (request.repository.clone(), request.number);
        if self.pull_request_diffs.contains_key(&key)
            || self.pull_request_diffs_loading.contains(&key)
        {
            return;
        }
        self.pull_request_diffs_loading.insert(key.clone());
        let entity = cx.entity().downgrade();
        let request_key = key.clone();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load_pull_request_diff(&request) })
                .await;
            let _ = entity.update(cx, |this, cx| {
                this.pull_request_diffs_loading.remove(&request_key);
                match result {
                    Ok(diff) => {
                        if let Some(entry) = this
                            .pull_requests
                            .iter_mut()
                            .find(|entry| entry.repository == request_key.0 && entry.number == request_key.1)
                        {
                            entry.cached_diff = Some(diff.1.clone());
                            save_cached_pull_requests(&this.pull_requests);
                        }
                        this.pull_request_diffs.insert(request_key.clone(), Arc::new(diff.0));
                        this.pull_request_diffs_error.remove(&request_key);
                    }
                    Err(error) => {
                        this.pull_request_diffs_error.insert(request_key, error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn ensure_pull_request_commits(
        &mut self,
        request: PullRequest,
        cx: &mut Context<Self>,
    ) {
        let key = (request.repository.clone(), request.number);
        if self.pull_request_commits.contains_key(&key)
            || self.pull_request_commits_loading.contains(&key)
        {
            return;
        }
        self.pull_request_commits_loading.insert(key.clone());
        let entity = cx.entity().downgrade();
        let request_key = key.clone();
        cx.spawn(async move |_, cx| {
            let result = cx
                .background_executor()
                .spawn(async move { load_pull_request_commits(&request) })
                .await;
            let _ = entity.update(cx, |this, cx| {
                this.pull_request_commits_loading.remove(&request_key);
                match result {
                    Ok(commits) => {
                        if let Some(entry) = this
                            .pull_requests
                            .iter_mut()
                            .find(|entry| entry.repository == request_key.0 && entry.number == request_key.1)
                        {
                            entry.cached_commits = commits.clone();
                            save_cached_pull_requests(&this.pull_requests);
                        }
                        this.pull_request_commits.insert(request_key.clone(), commits);
                        this.pull_request_commits_error.remove(&request_key);
                    }
                    Err(error) => {
                        this.pull_request_commits_error.insert(request_key, error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn render_pull_request_checks_section(
        &self,
        key: &(String, u64),
        theme: &Theme,
    ) -> AnyElement {
        let is_loading = self.pull_request_checks_loading.contains(key)
            || (!self.pull_request_checks.contains_key(key) && !self.pull_request_checks_error.contains_key(key));
        let content = if is_loading {
            div().text_color(theme.text_secondary).child("Loading checks…")
        } else if let Some(error) = self.pull_request_checks_error.get(key) {
            div().text_color(theme.danger).child(SharedString::from(error.clone()))
        } else if let Some(checks) = self.pull_request_checks.get(key) {
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .children(checks.iter().map(|check| {
                    let color = match check.bucket.as_str() {
                        "pass" => theme.success,
                        "fail" | "cancel" => theme.danger,
                        _ => theme.text_secondary,
                    };
                    div()
                        .w_full()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(icon(
                            if check.bucket == "pass" {
                                "icons/check.svg"
                            } else {
                                "icons/circle.svg"
                            },
                            14.0,
                            color,
                        ))
                        .child(div().min_w_0().flex_1().text_color(theme.text).child(SharedString::from(check.name.clone())))
                        .child(div().text_color(theme.text_secondary).child(SharedString::from(check.state.clone())))
                }))
        } else {
            div().text_color(theme.text_secondary).child("No checks reported.")
        };
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(div().text_size(sp(15.0)).font_weight(FontWeight::MEDIUM).text_color(theme.text).child("Checks"))
            .child(content)
            .into_any_element()
    }

    fn render_pull_request_comments_section(
        &self,
        key: &(String, u64),
        request: PullRequest,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let is_loading = self.pull_request_comments_loading.contains(key)
            || (!self.pull_request_comments.contains_key(key) && !self.pull_request_comments_error.contains_key(key));
        let comments = self.pull_request_comments.get(key);
        let comments_body = if is_loading {
            div().text_color(theme.text_secondary).child("Loading comments…").into_any_element()
        } else if let Some(error) = self.pull_request_comments_error.get(key) {
            div().text_color(theme.danger).child(SharedString::from(error.clone())).into_any_element()
        } else if let Some(comments) = comments {
            let palette = MarkdownPalette::from_theme(theme);
            div()
                .flex()
                .flex_col()
                .gap(px(10.0))
                .children(comments.iter().enumerate().map(|(index, comment)| {
                    let comment_key = format!("{}:{}:{index}", key.0, key.1);
                    let collapsed = self.pull_request_collapsed_comments.contains(&comment_key);
                    let entity = cx.entity().downgrade();
                    let toggle_key = comment_key.clone();
                    let header = div()
                        .id(SharedString::from(format!("pull-request-comment-header-{index}")))
                        .w_full()
                        .px(px(10.0))
                        .py(px(9.0))
                        .rounded(px(7.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .hover(|element| element.bg(theme.overlay_strong))
                        .focus_visible(|element| element.border_1().border_color(theme.accent))
                        .tab_index(0)
                        .on_click(move |_, _, cx| {
                            let _ = entity.update(cx, |this, cx| {
                                if !this.pull_request_collapsed_comments.remove(&toggle_key) {
                                    this.pull_request_collapsed_comments.insert(toggle_key.clone());
                                }
                                cx.notify();
                            });
                        })
                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                if !this.pull_request_collapsed_comments.remove(&comment_key) {
                                    this.pull_request_collapsed_comments.insert(comment_key.clone());
                                }
                                cx.stop_propagation();
                                cx.notify();
                            }
                        }))
                        .child(icon(
                            if collapsed { "icons/chevron-right.svg" } else { "icons/chevron-down.svg" },
                            13.0,
                            theme.text_tertiary,
                        ))
                        .child(div().min_w_0().flex_1().text_color(theme.text).font_weight(FontWeight::MEDIUM).child(SharedString::from(format!("{}  ·  {}", comment.author, comment.created_at))))
                        .when_some(comment.location.as_ref(), |element, location| {
                            element.child(div().text_color(theme.text_tertiary).child(SharedString::from(location.clone())))
                        });
                    let mut card = div().w_full().rounded(px(8.0)).bg(theme.overlay).child(header);
                    if !collapsed {
                        let body = without_html_comments(&comment.body);
                        let mut view = MarkdownView::new();
                        view.set_text(&body, false);
                        let context = MarkdownCtx::new(
                            format!("pull-request-comment-{}-{index}", key.1),
                            &palette,
                            MarkdownMetrics::document(self.state.ui_font_size, self.state.code_font_size),
                            self.transcript_selection.clone(),
                        )
                        .with_math_enabled(self.state.render_math)
                        .with_link_handler(self.markdown_link_handler.clone());
                        let rendered = md::render::markdown(&view, &context).unwrap_or_else(|| {
                            md::render::plain_text(body, crate::theme::active_ui_font_family(), FontWeight::NORMAL, theme.text_secondary, &context)
                        });
                        card = card.child(div().px(px(32.0)).pb(px(12.0)).child(rendered));
                    }
                    card
                }))
                .into_any_element()
        } else {
            div().text_color(theme.text_secondary).child("No comments yet.").into_any_element()
        };
        let entity = cx.entity().downgrade();
        let key_entity = entity.clone();
        let request = request.clone();
        let key_request = request.clone();
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(div().text_size(sp(15.0)).font_weight(FontWeight::MEDIUM).text_color(theme.text).child(SharedString::from(format!("Comments  ·  {}", comments.map_or(0, Vec::len)))))
            .child(comments_body)
            .child(
                div()
                    .w_full()
                    .min_h(px(44.0))
                    .px(px(10.0))
                    .py(px(7.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(theme.border_strong)
                    .bg(theme.inset)
                    .flex()
                    .items_end()
                    .gap(px(8.0))
                    .child(div().min_w_0().flex_1().child(self.pull_request_comment_input.clone()))
                    .child(
                        div()
                            .id("pull-request-submit-comment")
                            .size(px(28.0))
                            .rounded(px(7.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .when(self.pull_request_comment_posting, |element| element.opacity(0.5))
                            .hover(|element| element.bg(theme.overlay))
                            .focus_visible(|element| element.border_1().border_color(theme.accent))
                            .tab_index(0)
                            .child(icon("icons/arrow-up.svg", 15.0, theme.text))
                            .on_click(move |_, _, cx| {
                                let _ = entity.update(cx, |this, cx| {
                                    this.submit_pull_request_comment(request.clone(), cx);
                                });
                            })
                            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    let _ = key_entity.update(cx, |this, cx| {
                                        this.submit_pull_request_comment(key_request.clone(), cx);
                                    });
                                    cx.stop_propagation();
                                }
                            }),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_pull_requests(
        &self,
        _window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::current(cx);
        let selected_tab = self.pull_requests_tab;
        let query = self
            .pull_requests_search
            .read(cx)
            .content()
            .trim()
            .to_ascii_lowercase();
        let entries = self
            .pull_requests
            .iter()
            .filter(|entry| match selected_tab {
                PullRequestTab::All => true,
                PullRequestTab::Open => entry.state == "open",
                PullRequestTab::Closed => entry.state == "closed",
                PullRequestTab::Merged => entry.state == "merged",
            })
            .filter(|entry| {
                query.is_empty()
                    || entry.title.to_ascii_lowercase().contains(&query)
                    || entry.repository.to_ascii_lowercase().contains(&query)
            })
            .collect::<Vec<_>>();
        let search = TextField::new("pull-requests-search", self.pull_requests_search.clone())
            .icon("icons/search.svg", 15.0)
            .flex_1()
            .max_w(px(760.0));
        let pull_requests = cx.entity().downgrade();
        let refresh = cx.entity().downgrade();

        div()
            .size_full()
            .flex()
            .flex_col()
            .min_h_0()
            .bg(theme.surface)
            .child(
                div()
                    .flex_none()
                    .px(px(32.0))
                    .pt(px(18.0))
                    .pb(px(12.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .children(PullRequestTab::ALL.into_iter().map(|tab| {
                        let selected = tab == selected_tab;
                        let click_pull_requests = pull_requests.clone();
                        let key_pull_requests = pull_requests.clone();
                        div()
                            .id(SharedString::from(format!(
                                "pull-requests-tab-{}",
                                tab.label()
                            )))
                            .tab_index(0)
                            .h(px(32.0))
                            .px(px(14.0))
                            .rounded(px(7.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_default()
                            .text_size(sp(13.0))
                            .font_weight(if selected {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if selected {
                                theme.text
                            } else {
                                theme.text_secondary
                            })
                            .border_1()
                            .border_color(if selected {
                                theme.border
                            } else {
                                gpui::transparent_black()
                            })
                            .when(selected, |element| element.bg(theme.overlay_strong))
                            .when(!selected, |element| {
                                element.hover(|element| {
                                    element.bg(theme.overlay).text_color(theme.text)
                                })
                            })
                            .active(|element| element.bg(theme.overlay_strong))
                            .on_click(move |_, _, cx| {
                                let _ = click_pull_requests.update(cx, |this, cx| {
                                    this.pull_requests_tab = tab;
                                    this.pull_requests_scroll_handle.set_offset(Point::default());
                                    cx.notify();
                                });
                            })
                            .on_key_down(move |event: &KeyDownEvent, _, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    let _ = key_pull_requests.update(cx, |this, cx| {
                                        this.pull_requests_tab = tab;
                                        this.pull_requests_scroll_handle.set_offset(Point::default());
                                        cx.notify();
                                    });
                                    cx.stop_propagation();
                                }
                            })
                            .child(tab.label())
                    })),
            )
            .child(
                div()
                    .flex_none()
                    .px(px(32.0))
                    .pb(px(18.0))
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(search)
                    .child(
                        div()
                            .id("pull-requests-refresh")
                            .size(px(34.0))
                            .rounded(px(8.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .hover(|element| element.bg(theme.overlay))
                            .active(|element| element.bg(theme.overlay_strong))
                            .tooltip(Tooltip::text("Refresh pull requests"))
                            .child(icon("icons/rotate-cw.svg", 15.0, theme.text_tertiary))
                            .on_click(move |_, _, cx| {
                                let _ = refresh.update(cx, |this, cx| {
                                    this.ensure_pull_requests(true, cx);
                                });
                            }),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(
                        div()
                            .id("pull-requests-list")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.pull_requests_scroll_handle)
                            .px(px(32.0))
                            .pb(px(48.0))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .children(if self.pull_requests_loading {
                                let loading_style = ShimmerStyle::new()
                                    .duration(Duration::from_secs(3))
                                    .highlight_color(cx.theme().primary)
                                    .spread(0.45)
                                    .reverse(true)
                                    .once(false);

                                vec![
                                    div()
                                        .w_full()
                                        .py(px(80.0))
                                        .flex()
                                        .flex_col()
                                        .items_center()
                                        .justify_center()
                                        .gap(px(14.0))
                                        .child(crate::app::components::dot_matrix_loader(
                                            theme.text_secondary,
                                            22.0,
                                        ))
                                        .child(
                                            ShimmerText::new("Loading pull requests…")
                                                .with_shimmer_style(loading_style)
                                                .text_size(sp(14.0))
                                                .text_color(theme.text_secondary),
                                        )
                                        .into_any_element(),
                                ]
                            } else if let Some(error) = &self.pull_requests_error {
                                vec![
                                    div()
                                        .py(px(40.0))
                                        .text_color(theme.danger)
                                        .child(SharedString::from(error.clone()))
                                        .into_any_element(),
                                ]
                            } else if entries.is_empty() {
                                vec![
                                    div()
                                        .py(px(40.0))
                                        .text_color(theme.text_secondary)
                                        .child(format!(
                                            "No {} pull requests found",
                                            selected_tab.label().to_lowercase()
                                        ))
                                        .into_any_element(),
                                ]
                            } else {
                                let selected_key = self
                                    .pull_request_detail
                                    .as_ref()
                                    .map(|detail| (detail.repository.clone(), detail.number));
                                entries
                                    .into_iter()
                                    .map(|entry| {
                                        let is_selected = selected_key.as_ref()
                                            == Some(&(entry.repository.clone(), entry.number));
                                        render_pull_request_row(
                                            entry,
                                            theme,
                                            is_selected,
                                            cx.entity().downgrade(),
                                        )
                                    })
                                    .collect()
                            }),
                    )
                    .child(scrollbar::vertical(
                        &self.pull_requests_scroll_handle,
                        &self.pull_requests_scrollbar,
                    )),
            )
            .into_any_element()
    }
}

fn render_pull_request_row(
    entry: &PullRequest,
    theme: Theme,
    is_selected: bool,
    insulator: WeakEntity<Insulator>,
) -> AnyElement {
    let state_color = match entry.state.as_str() {
        "open" => theme.success,
        "merged" => theme.accent,
        "closed" => theme.danger,
        _ => theme.text_secondary,
    };
    let request = entry.clone();
    div()
        .id(SharedString::from(format!(
            "pull-request-{}-{}",
            entry.repository, entry.number
        )))
        .w_full()
        .max_w(px(980.0))
        .rounded(px(8.0))
        .cursor_pointer()
        .px(px(14.0))
        .py(px(12.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .when(is_selected, |element| {
            element
                .bg(theme.overlay_strong)
                .border_1()
                .border_color(theme.border)
                .hover(|element| element.bg(theme.overlay_strong))
        })
        .when(!is_selected, |element| {
            element
                .border_1()
                .border_color(gpui::transparent_black())
                .hover(|element| {
                    element
                        .bg(theme.overlay)
                        .border_color(theme.border)
                })
                .active(|element| element.bg(theme.overlay_strong))
        })
        .on_click(move |_, _, cx| {
            let _ = insulator.update(cx, |this, cx| {
                this.pull_request_detail = Some(request.clone());
                this.pull_request_comment_input.update(cx, |input, cx| input.clear(cx));
                this.pull_request_detail_tab = PullRequestDetailTab::Summary;
                this.pull_request_detail_scroll_handle.set_offset(Point::default());
                this.ensure_pull_request_body(request.clone(), cx);
                this.ensure_pull_request_commits(request.clone(), cx);
                this.ensure_pull_request_checks(request.clone(), cx);
                this.ensure_pull_request_comments(request.clone(), cx);
                this.set_right_panel_visible(true, cx);
            });
        })
        .child(
            div()
                .w(px(3.0))
                .h(px(28.0))
                .rounded(px(2.0))
                .flex_none()
                .bg(if is_selected {
                    theme.accent
                } else {
                    gpui::transparent_black()
                }),
        )
        .child(icon("icons/git-branch.svg", 17.0, state_color))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(sp(14.0))
                        .font_weight(if is_selected {
                            FontWeight::MEDIUM
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(theme.text)
                        .child(SharedString::from(format!(
                            "{}  #{}",
                            entry.title, entry.number
                        ))),
                )
                .child(
                    div()
                        .text_size(sp(12.5))
                        .text_color(theme.text_secondary)
                        .child(SharedString::from(format!(
                            "{}  ·  {}  ·  {}",
                            entry.repository,
                            if entry.author.is_empty() {
                                "unknown"
                            } else {
                                &entry.author
                            },
                            entry.state
                        ))),
                ),
        )
        .child(
            div()
                .flex_none()
                .px(px(9.0))
                .py(px(3.5))
                .rounded(px(999.0))
                .bg(state_color.opacity(if theme.is_dark { 0.18 } else { 0.12 }))
                .text_size(sp(11.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(state_color)
                .child(SharedString::from(entry.state.to_ascii_uppercase())),
        )
        .into_any_element()
}

impl Insulator {
    pub(super) fn render_pull_request_detail_panel(
        &mut self,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::current(cx);
        let Some(request) = self.pull_request_detail.as_ref() else {
            return div().id("pull-request-detail-empty").w(px(width)).h_full();
        };
        let close = cx.entity().downgrade();
        let selected_tab = self.pull_request_detail_tab;
        let detail_entity = cx.entity().downgrade();
        let commit_key = (request.repository.clone(), request.number);
        let commits = self.pull_request_commits.get(&commit_key);
        let github_url = if request.url.is_empty() {
            format!(
                "https://github.com/{}/pull/{}",
                request.repository, request.number
            )
        } else {
            request.url.clone()
        };
        let body = without_html_comments(&request.body);
        let loading_style = ShimmerStyle::new()
            .duration(Duration::from_secs(3))
            .highlight_color(cx.theme().primary)
            .spread(0.45)
            .reverse(true)
            .once(false);

        let is_body_loading = !request.body_loaded
            && !self.pull_request_body_error.contains_key(&commit_key)
            || self.pull_request_detail_loading.contains(&commit_key);

        let description = if let Some(error) = self.pull_request_body_error.get(&commit_key) {
            div()
                .text_color(theme.danger)
                .child(SharedString::from(error.clone()))
                .into_any_element()
        } else if is_body_loading {
            div()
                .w_full()
                .py(px(60.0))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(14.0))
                .child(crate::app::components::dot_matrix_loader(
                    theme.text_secondary,
                    20.0,
                ))
                .child(
                    ShimmerText::new("Loading description…")
                        .with_shimmer_style(loading_style)
                        .text_size(sp(13.5))
                        .text_color(theme.text_secondary),
                )
                .into_any_element()
        } else if body.is_empty() {
            div()
                .text_color(theme.text_secondary)
                .child("No description provided.")
                .into_any_element()
        } else {
            let key = (request.repository.clone(), request.number);
            let mut cache = self.pull_request_markdown.borrow_mut();
            if !matches!(cache.as_ref(), Some((cached_key, _)) if cached_key == &key) {
                *cache = Some((key.clone(), MarkdownView::new()));
            }
            let (_, view) = cache.as_mut().expect("markdown cache initialized");
            view.set_text(&body, false);
            let palette = MarkdownPalette::from_theme(&theme);
            let markdown_context = MarkdownCtx::new(
                format!(
                    "pull-request-description-{}-{}",
                    request.repository, request.number
                ),
                &palette,
                MarkdownMetrics::document(self.state.ui_font_size, self.state.code_font_size),
                self.transcript_selection.clone(),
            )
            .with_math_enabled(self.state.render_math)
            .with_link_handler(self.markdown_link_handler.clone());
            md::render::markdown(view, &markdown_context).unwrap_or_else(|| {
                md::render::plain_text(
                    body,
                    crate::theme::active_ui_font_family(),
                    FontWeight::NORMAL,
                    theme.text,
                    &markdown_context,
                )
            })
        };
        let detail_body = match selected_tab {
            PullRequestDetailTab::Summary => div()
                .flex()
                .flex_col()
                .gap(px(18.0))
                .child(
                    div()
                        .text_size(sp(20.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(SharedString::from(request.title.clone())),
                )
                .child(
                    div()
                        .text_size(sp(13.0))
                        .text_color(theme.text_secondary)
                        .child(SharedString::from(format!(
                            "{}  ·  {}",
                            request.repository, request.state
                        ))),
                )
                .child(div().h(px(1.0)).w_full().bg(theme.border))
                .child(
                    div()
                        .text_size(sp(14.0))
                        .line_height(sp(22.0))
                        .text_color(theme.text_secondary)
                        .child(description),
                )
                .child(self.render_pull_request_checks_section(&commit_key, &theme))
                .child(self.render_pull_request_comments_section(
                    &commit_key,
                    request.clone(),
                    &theme,
                    cx,
                ))
                .into_any_element(),
            PullRequestDetailTab::Commits => {
                let is_commits_loading = self.pull_request_commits_loading.contains(&commit_key)
                    || (!self.pull_request_commits.contains_key(&commit_key)
                        && !self.pull_request_commits_error.contains_key(&commit_key));
                let content = if is_commits_loading {
                    div()
                        .w_full()
                        .py(px(60.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(14.0))
                        .child(crate::app::components::dot_matrix_loader(
                            theme.text_secondary,
                            20.0,
                        ))
                        .child(
                            ShimmerText::new("Loading commits…")
                                .with_shimmer_style(loading_style)
                                .text_size(sp(13.5))
                                .text_color(theme.text_secondary),
                        )
                } else if let Some(error) = self.pull_request_commits_error.get(&commit_key) {
                    div()
                        .text_color(theme.danger)
                        .child(SharedString::from(error.clone()))
                } else if let Some(commits) = commits {
                    if commits.is_empty() {
                        div()
                            .text_color(theme.text_secondary)
                            .child("No commits found.")
                    } else {
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .children(commits.iter().map(|commit| {
                                div()
                                    .px(px(10.0))
                                    .py(px(9.0))
                                    .rounded(px(7.0))
                                    .bg(theme.overlay)
                                    .child(
                                        div()
                                            .text_size(sp(13.0))
                                            .text_color(theme.text)
                                            .child(SharedString::from(commit.message.clone())),
                                    )
                                    .child(
                                        div()
                                            .mt(px(4.0))
                                            .text_size(sp(11.0))
                                            .text_color(theme.text_secondary)
                                            .child(SharedString::from(format!(
                                                "{}  ·  {}",
                                                &commit.oid[..commit.oid.len().min(7)],
                                                commit.authored_at
                                            ))),
                                    )
                            }))
                    }
                } else {
                    div()
                        .text_color(theme.text_secondary)
                        .child("No commits found.")
                };
                content.into_any_element()
            }
            PullRequestDetailTab::Code => {
                let is_diffs_loading = self.pull_request_diffs_loading.contains(&commit_key)
                    || (!self.pull_request_diffs.contains_key(&commit_key)
                        && !self.pull_request_diffs_error.contains_key(&commit_key));
                if is_diffs_loading {
                    div()
                        .size_full()
                        .py(px(80.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(14.0))
                        .child(crate::app::components::dot_matrix_loader(
                            theme.text_secondary,
                            22.0,
                        ))
                        .child(
                            ShimmerText::new("Loading changes…")
                                .with_shimmer_style(loading_style)
                                .text_size(sp(13.5))
                                .text_color(theme.text_secondary),
                        )
                        .into_any_element()
                } else if let Some(error) = self.pull_request_diffs_error.get(&commit_key) {
                    div()
                        .text_color(theme.danger)
                        .child(SharedString::from(error.clone()))
                        .into_any_element()
                } else if let Some(diff) = self.pull_request_diffs.get(&commit_key) {
                    if diff.files.is_empty() {
                        div()
                            .text_color(theme.text_secondary)
                            .child("No code changes found.")
                            .into_any_element()
                    } else {
                        if !self
                            .right_panel_diff_snapshot
                            .as_ref()
                            .is_some_and(|current| Arc::ptr_eq(current, diff))
                        {
                            self.right_panel_diff_snapshot = Some(diff.clone());
                            self.right_panel_diff_list_state.reset(diff.lines.len());
                        }
                        self.render_right_panel_unified_diff(diff.clone(), cx)
                            .into_any_element()
                    }
                } else {
                    div()
                        .text_color(theme.text_secondary)
                        .child("No code changes found.")
                        .into_any_element()
                }
            }
        };
        div()
            .id("pull-request-detail-panel")
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .min_w_0()
            .border_l_1()
            .border_color(theme.border_strong)
            .bg(theme.surface)
            .child(
                div()
                    .h(px(48.0))
                    .flex_none()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme.border)
                    .child(
                        div()
                            .px(px(10.0))
                            .h(px(30.0))
                            .rounded(px(8.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .bg(theme.overlay)
                            .text_color(theme.text)
                            .child(icon("icons/git-branch.svg", 15.0, theme.accent))
                            .child(format!("PR #{}", request.number)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child({
                                let url = github_url.clone();
                                div()
                                    .id("open-pull-request-github")
                                    .size(px(26.0))
                                    .rounded(px(6.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(|element| element.bg(theme.overlay))
                                    .tooltip(Tooltip::text("Open on GitHub"))
                                    .child(icon(
                                        "icons/external-link.svg",
                                        14.0,
                                        theme.text_secondary,
                                    ))
                                    .on_click(move |_, _, cx| cx.open_url(&url))
                            })
                            .child(
                                div()
                                    .id("close-pull-request-detail")
                                    .size(px(26.0))
                                    .rounded(px(6.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .hover(|element| element.bg(theme.overlay))
                                    .child(icon("icons/x.svg", 14.0, theme.text_secondary))
                                    .on_click(move |_, _, cx| {
                                        let _ = close.update(cx, |this, cx| {
                                            this.pull_request_detail = None;
                                            this.set_right_panel_visible(false, cx);
                                        });
                                    }),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .h(px(46.0))
                    .px(px(16.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .children(PullRequestDetailTab::ALL.into_iter().map(|tab| {
                        let active = tab == selected_tab;
                        let detail_entity = detail_entity.clone();
                        let request_for_tab = request.clone();
                        div()
                            .id(SharedString::from(format!(
                                "pull-request-detail-tab-{}",
                                tab.label()
                            )))
                            .tab_index(0)
                            .h(px(32.0))
                            .px(px(12.0))
                            .rounded(px(7.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_default()
                            .text_size(sp(13.0))
                            .font_weight(if active {
                                FontWeight::MEDIUM
                            } else {
                                FontWeight::NORMAL
                            })
                            .text_color(if active {
                                theme.text
                            } else {
                                theme.text_secondary
                            })
                            .border_1()
                            .border_color(if active {
                                theme.border
                            } else {
                                gpui::transparent_black()
                            })
                            .when(active, |element| element.bg(theme.overlay_strong))
                            .when(!active, |element| {
                                element.hover(|element| {
                                    element.bg(theme.overlay).text_color(theme.text)
                                })
                            })
                            .active(|element| element.bg(theme.overlay_strong))
                            .on_click(move |_, _, cx| {
                                let _ = detail_entity.update(cx, |this, cx| {
                                    this.pull_request_detail_tab = tab;
                                    this.pull_request_detail_scroll_handle.set_offset(Point::default());
                                    if tab == PullRequestDetailTab::Commits {
                                        this.ensure_pull_request_commits(
                                            request_for_tab.clone(),
                                            cx,
                                        );
                                    } else if tab == PullRequestDetailTab::Code {
                                        this.ensure_pull_request_diff(request_for_tab.clone(), cx);
                                    }
                                    cx.notify();
                                });
                            })
                            .child(tab.label())
                    })),
            )
            .child(if selected_tab == PullRequestDetailTab::Code {
                div()
                    .id("pull-request-changes")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .child(detail_body)
            } else {
                div()
                    .id("pull-request-detail-scroll-pane")
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(
                        div()
                            .id("pull-request-detail-scroll")
                            .size_full()
                            .overflow_y_scroll()
                            .track_scroll(&self.pull_request_detail_scroll_handle)
                            .px(px(24.0))
                            .py(px(24.0))
                            .pb(px(48.0))
                            .child(detail_body),
                    )
                    .child(scrollbar::vertical(
                        &self.pull_request_detail_scroll_handle,
                        &self.pull_request_detail_scrollbar,
                    ))
            })
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn gh_fields_include_the_list_data() {
        assert!(super::GH_FIELDS.contains("repository"));
        assert!(super::GH_FIELDS.contains("updatedAt"));
    }

    #[test]
    fn generated_html_is_removed_from_descriptions() {
        assert_eq!(
            super::without_html_comments("<!-- generated -->\n## Summary\n<p>Done</p>"),
            "## Summary\nDone"
        );
    }

    #[test]
    fn refresh_preserves_cached_pull_request_details() {
        let cached = super::PullRequest {
            number: 1,
            title: "Old title".into(),
            author: "contributor".into(),
            body: "Cached body".into(),
            body_loaded: true,
            repository: "owner/repo".into(),
            state: "open".into(),
            url: "https://github.com/owner/repo/pull/1".into(),
            updated_at: String::new(),
            cached_commits: vec![super::PullRequestCommit {
                oid: "abc".into(),
                message: "Commit".into(),
                authored_at: String::new(),
            }],
            cached_diff: Some("patch".into()),
        };
        let mut refreshed = vec![super::PullRequest {
            title: "New title".into(),
            body: String::new(),
            body_loaded: false,
            url: String::new(),
            cached_commits: Vec::new(),
            cached_diff: None,
            ..cached.clone()
        }];

        super::preserve_cached_details(&mut refreshed, &[cached]);

        assert_eq!(refreshed[0].body, "Cached body");
        assert_eq!(refreshed[0].cached_commits.len(), 1);
        assert_eq!(refreshed[0].cached_diff.as_deref(), Some("patch"));
    }
}
