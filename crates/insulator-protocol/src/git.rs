use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::model::ProviderKind;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct BranchEntry {
    pub name: String,
    pub checked_out_elsewhere: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct BranchSnapshot {
    #[ts(type = "string")]
    pub repository: PathBuf,
    pub current: Option<String>,
    pub detached_head: Option<String>,
    pub default_branch: Option<String>,
    /// `None` means there is no current branch (detached HEAD), or the GitHub lookup failed or timed out.
    #[serde(default)]
    pub pull_request_open: Option<bool>,
    pub branches: Vec<BranchEntry>,
    pub additions: u64,
    pub deletions: u64,
}

impl BranchSnapshot {
    pub fn display_branch(&self) -> Option<&str> {
        self.current.as_deref().or(self.detached_head.as_deref())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct CommitSnapshot {
    pub branch: String,
    pub additions: u64,
    pub deletions: u64,
    pub staged_additions: u64,
    pub staged_deletions: u64,
    pub has_staged: bool,
    pub has_unstaged: bool,
    pub can_push: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub struct AgentInvocation {
    pub provider: ProviderKind,
    #[ts(type = "string")]
    pub binary: PathBuf,
    pub model: Option<String>,
    pub reasoning_effort: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct CreatedWorktree {
    #[ts(type = "string")]
    pub path: PathBuf,
    pub branch: String,
}

/// How a changed file differs from `HEAD`, as `git status` letters it.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum FileChangeStatus {
    Modified,
    Added,
    Deleted,
    Untracked,
}

/// One uncommitted file. A partly staged file has both `staged` and
/// `unstaged` set; its counts cover both halves.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct FileChange {
    pub path: String,
    pub status: FileChangeStatus,
    pub staged: bool,
    pub unstaged: bool,
    pub additions: u64,
    pub deletions: u64,
}

/// A working copy's uncommitted files and where its branch stands against
/// its upstream and the default branch.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct WorkingTreeStatus {
    pub branch: String,
    pub default_branch: Option<String>,
    pub has_remote: bool,
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Commits on this branch that the default branch does not have.
    pub ahead_of_default: u32,
    pub files: Vec<FileChange>,
}

/// A checkout of the repository, as `git worktree list` reports it.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, TS)]
pub struct WorktreeEntry {
    #[ts(type = "string")]
    pub path: PathBuf,
    /// `None` for a detached HEAD.
    pub branch: Option<String>,
    /// The repository's main checkout rather than a linked worktree.
    pub is_main: bool,
}
