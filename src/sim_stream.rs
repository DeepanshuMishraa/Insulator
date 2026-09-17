//! iOS Simulator streaming via `serve-sim`, T3 Code-style.
//!
//! Thin tracer bullet: own the `serve-sim` helper lifecycle for one explicit
//! simulator UDID and open its local preview URL in the existing in-app
//! [`crate::browser::BrowserView`]. Semantic driving stays with screenshots /
//! a future XcodeBuildMCP hook; the stream is the shared visual feed.
//!
//! All subprocess work here is blocking and must run on
//! `cx.background_executor()`, never in `render`. Frames only read cached
//! state stored on `Insulator`.

use std::fmt;
use std::process::Command;
use std::time::{Duration, Instant};

/// Pinned `serve-sim` release, matching T3 Code's
/// `.agents/skills/ios-simulator-browser/SKILL.md`.
pub const SERVE_SIM_PACKAGE: &str = "serve-sim@0.1.45";
/// `serve-sim` must stay on loopback: the preview exposes a token-gated
/// shell-execution route.
pub const SERVE_SIM_HOST: &str = "127.0.0.1";
/// Default simulator: a plain iPhone, never an iPad.
pub const DEFAULT_SIMULATOR_NAME: &str = "iPhone 18 Pro";

/// Preview appearance, mirrored from the app theme so the stream does not
/// clash with the surrounding panel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimTheme {
    Dark,
    Light,
}

impl SimTheme {
    fn flag(self) -> &'static str {
        match self {
            Self::Dark => "dark",
            Self::Light => "light",
        }
    }
}

/// Why simulator streaming cannot run on this host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimSupport {
    Supported,
    UnsupportedPlatform,
    UnsupportedArch,
}

/// Gate: Apple Silicon macOS only (`serve-sim-bin` ships arm64-only).
pub fn support() -> SimSupport {
    if !cfg!(target_os = "macos") {
        return SimSupport::UnsupportedPlatform;
    }
    if std::env::consts::ARCH != "aarch64" {
        return SimSupport::UnsupportedArch;
    }
    SimSupport::Supported
}

/// Tagged failures for simulator streaming. Display strings are user-facing:
// what happened + what to do next. No secrets are interpolated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimStreamError {
    UnsupportedPlatform,
    UnsupportedArch,
    MissingTool(&'static str),
    InvalidUdid,
    NoDevices,
    CommandFailed {
        tool: &'static str,
        detail: String,
    },
    ParseFailed {
        tool: &'static str,
        detail: String,
    },
}

impl fmt::Display for SimStreamError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                write!(f, "iOS Simulator streaming needs macOS with Xcode installed")
            }
            Self::UnsupportedArch => {
                write!(
                    f,
                    "iOS Simulator streaming needs Apple Silicon (serve-sim is arm64-only)"
                )
            }
            Self::MissingTool(tool) => {
                write!(f, "{tool} is not installed; install Xcode command-line tools and Node.js 20+")
            }
            Self::InvalidUdid => write!(f, "invalid simulator UDID"),
            Self::NoDevices => write!(
                f,
                "no iOS simulators found; install an iOS runtime in Xcode first"
            ),
            Self::CommandFailed { tool, detail } => {
                write!(f, "{tool} failed: {detail}")
            }
            Self::ParseFailed { tool, detail } => {
                write!(f, "could not parse {tool} output: {detail}")
            }
        }
    }
}

impl std::error::Error for SimStreamError {}

/// One simulator from `xcrun simctl list devices -j`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimDevice {
    pub udid: String,
    pub name: String,
    pub state: String,
    pub is_available: bool,
}

impl SimDevice {
    pub fn is_booted(&self) -> bool {
        self.state.eq_ignore_ascii_case("booted")
    }
}

/// Running `serve-sim` helper from `--detach -q` / `--list -q`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimStreamInfo {
    pub device: String,
    pub url: String,
    pub stream_url: String,
    pub ws_url: String,
    pub port: u16,
    pub pid: Option<u32>,
}

/// UDIDs are `8-4-4-4-12` hex; reject anything else before it reaches a shell.
pub fn validate_udid(udid: &str) -> Result<(), SimStreamError> {
    let parts: Vec<&str> = udid.split('-').collect();
    let lens = [8usize, 4, 4, 4, 12];
    if parts.len() != lens.len() {
        return Err(SimStreamError::InvalidUdid);
    }
    let ok = parts
        .iter()
        .zip(lens)
        .all(|(part, len)| part.len() == len && part.bytes().all(|b| b.is_ascii_hexdigit()));
    if ok {
        Ok(())
    } else {
        Err(SimStreamError::InvalidUdid)
    }
}

/// Pick the default stream target: iPhones over iPads, the default iPhone
/// first, already-booted preferred within ties. One explicit UDID is then
/// pinned for the whole stream session (T3 parity).
pub fn pick_preferred(devices: &[SimDevice]) -> Option<SimDevice> {
    /// Lower is better: iPhone 0, everything else 1 — iPads never win ties.
    fn family_rank(name: &str) -> u8 {
        if name.starts_with("iPhone") {
            0
        } else {
            1
        }
    }
    devices
        .iter()
        .filter(|d| d.is_available)
        .min_by_key(|d| {
            (
                family_rank(&d.name),
                if d.name == DEFAULT_SIMULATOR_NAME {
                    0
                } else {
                    1
                },
                if d.is_booted() { 0 } else { 1 },
                d.name.clone(),
            )
        })
        .cloned()
}

#[derive(serde::Deserialize)]
struct SimctlList {
    devices: std::collections::HashMap<String, Vec<SimctlDevice>>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SimctlDevice {
    #[serde(default)]
    udid: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    is_available: bool,
}

/// Parse `xcrun simctl list devices -j` stdout. Unknown runtimes are kept;
/// unavailable devices are kept (caller filters) so "none available" can be
/// distinguished from "none installed".
pub fn parse_simctl_devices(stdout: &str) -> Result<Vec<SimDevice>, SimStreamError> {
    let parsed: SimctlList = serde_json::from_str(stdout).map_err(|error| {
        SimStreamError::ParseFailed {
            tool: "simctl",
            detail: error.to_string(),
        }
    })?;
    let mut devices = Vec::new();
    for list in parsed.devices.values() {
        for device in list {
            if device.udid.is_empty() {
                continue;
            }
            devices.push(SimDevice {
                udid: device.udid.clone(),
                name: device.name.clone(),
                state: device.state.clone(),
                is_available: device.is_available,
            });
        }
    }
    devices.sort_by(|a, b| a.name.cmp(&b.name).then(a.udid.cmp(&b.udid)));
    Ok(devices)
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct ServeSimInfo {
    #[serde(default)]
    device: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    stream_url: String,
    #[serde(default)]
    ws_url: String,
    #[serde(default)]
    port: u16,
    #[serde(default)]
    pid: Option<u32>,
}

fn to_info(parsed: ServeSimInfo) -> Result<SimStreamInfo, SimStreamError> {
    if parsed.device.is_empty() || parsed.url.is_empty() {
        return Err(SimStreamError::ParseFailed {
            tool: "serve-sim",
            detail: "missing device or url".to_owned(),
        });
    }
    // The preview navigates to `url`, and the page exposes a token-gated
    // shell-execution route — so a compromised/mismatched helper must not be
    // able to redirect the webview off loopback. Reject anything that is not
    // `http` on `SERVE_SIM_HOST` before storing.
    validate_serve_url(&parsed.url, "http")?;
    if !parsed.stream_url.is_empty() {
        validate_serve_url(&parsed.stream_url, "http")?;
    }
    if !parsed.ws_url.is_empty() {
        validate_serve_url(&parsed.ws_url, "ws")?;
    }
    Ok(SimStreamInfo {
        device: parsed.device,
        url: parsed.url,
        stream_url: parsed.stream_url,
        ws_url: parsed.ws_url,
        port: parsed.port,
        pid: parsed.pid,
    })
}

/// Reject helper URLs that are not on the loopback preview host: the
/// Simulator tab navigates to `url`, so anything off-host (or non-HTTP(S)/WS)
/// would hand the token-gated shell route to an untrusted origin.
fn validate_serve_url(raw: &str, scheme: &str) -> Result<(), SimStreamError> {
    let invalid = || SimStreamError::ParseFailed {
        tool: "serve-sim",
        detail: "unexpected serve-sim url".to_owned(),
    };
    let parsed = url::Url::parse(raw).map_err(|_| invalid())?;
    if parsed.scheme() != scheme {
        return Err(invalid());
    }
    if parsed.host_str() != Some(SERVE_SIM_HOST) {
        return Err(invalid());
    }
    Ok(())
}

/// `String::truncate` panics when the cut falls inside a multibyte char;
/// floor to the char boundary first so long non-ASCII output cannot crash a
/// sim operation (and skip its normal error cleanup).
fn truncate_to_char_boundary(detail: &mut String, max_len: usize) {
    if detail.len() > max_len {
        let end = detail.floor_char_boundary(max_len);
        detail.truncate(end);
    }
}

/// Parse `serve-sim --detach -q <udid>` stdout: one JSON object.
pub fn parse_detach_output(stdout: &str) -> Result<SimStreamInfo, SimStreamError> {
    let parsed: ServeSimInfo = serde_json::from_str(stdout.trim()).map_err(|error| {
        SimStreamError::ParseFailed {
            tool: "serve-sim",
            detail: error.to_string(),
        }
    })?;
    to_info(parsed)
}

/// Parse `serve-sim --list -q [udid]` stdout: one object or an array.
/// An empty array means no stream for that device.
pub fn parse_list_output(stdout: &str) -> Result<Vec<SimStreamInfo>, SimStreamError> {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }
    if let Ok(list) = serde_json::from_str::<Vec<ServeSimInfo>>(trimmed) {
        return list.into_iter().map(to_info).collect();
    }
    let single: ServeSimInfo =
        serde_json::from_str(trimmed).map_err(|error| SimStreamError::ParseFailed {
            tool: "serve-sim",
            detail: error.to_string(),
        })?;
    // `--list` prints `{"running":false,...}` when idle on some versions.
    if single.url.is_empty() {
        return Ok(Vec::new());
    }
    Ok(vec![to_info(single)?])
}

fn command_failed(tool: &'static str, output: &std::process::Output) -> SimStreamError {
    let mut detail = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    if detail.is_empty() {
        detail = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    }
    if detail.is_empty() {
        detail = output
            .status
            .code()
            .map(|code| format!("exit {code}"))
            .unwrap_or_else(|| "failed".to_owned());
    }
    // Cap stderr passthrough so a long simctl dump cannot flood the toast.
    truncate_to_char_boundary(&mut detail, 300);
    SimStreamError::CommandFailed { tool, detail }
}

fn run(tool: &'static str, mut command: Command) -> Result<String, SimStreamError> {
    let output = command.output().map_err(|_| SimStreamError::MissingTool(tool))?;
    if !output.status.success() {
        return Err(command_failed(tool, &output));
    }
    String::from_utf8(output.stdout).map_err(|_| SimStreamError::ParseFailed {
        tool,
        detail: "non-utf8 output".to_owned(),
    })
}

// -- Blocking helpers: call only from `background_executor`. --

/// `xcrun simctl list devices -j`.
pub fn list_devices_blocking() -> Result<Vec<SimDevice>, SimStreamError> {
    let mut command = Command::new("xcrun");
    command.args(["simctl", "list", "devices", "-j"]);
    parse_simctl_devices(&run("simctl", command)?)
}

/// `xcrun simctl boot <udid>`; already-booted is success.
///
/// `simctl` reports that as `Unable to boot device in current state: Booted`
/// (not "already booted"), so match the state suffix — the old substring
/// guards never fired and every already-booted start surfaced as a failure.
pub fn boot_device_blocking(udid: &str) -> Result<(), SimStreamError> {
    validate_udid(udid)?;
    let mut command = Command::new("xcrun");
    command.args(["simctl", "boot", udid]);
    match run("simctl", command) {
        Ok(_) => Ok(()),
        Err(SimStreamError::CommandFailed { detail, .. })
            if is_already_booted(&detail) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// Real `simctl` failure text names the current state
/// (`... in current state: Booted`), while older Xcode prints
/// "already booted" — accept either, case-insensitively. The bare
/// "Invalid device state" substring is deliberately not matched: it also
/// fires for states that are genuine failures.
fn is_already_booted(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    lower.contains("already booted") || lower.contains("current state: booted")
}

/// Mirror of [`is_already_booted`] for shutdown:
/// `Unable to shutdown device in current state: Shutdown`.
fn is_already_shutdown(detail: &str) -> bool {
    let lower = detail.to_ascii_lowercase();
    // "already shut" covers "already shutdown" / "already shut down".
    lower.contains("already shut")
        || lower.contains("current state: shutdown")
        || lower.contains("current state: shut down")
}

/// `npx --yes serve-sim@0.1.45 --detach -q <udid>`; prints the helper URL.
/// Scoped to one UDID: never an unscoped `--kill`, never another task's stream.
///
/// The preview opens minimal on purpose: no side panes, fitted to the
/// viewport, themed like the app — the stream is the content, not a dashboard.
pub fn start_detached_blocking(udid: &str, theme: SimTheme) -> Result<SimStreamInfo, SimStreamError> {
    validate_udid(udid)?;
    let mut command = Command::new("npx");
    command.args([
        "--yes",
        SERVE_SIM_PACKAGE,
        "--detach",
        "-q",
        "--panes",
        "none",
        "--fit",
        "--theme",
        theme.flag(),
        udid,
    ]);
    parse_detach_output(&run("serve-sim", command)?)
}

/// `npx --yes serve-sim@0.1.45 --list -q [udid]`.
pub fn list_streams_blocking(udid: Option<&str>) -> Result<Vec<SimStreamInfo>, SimStreamError> {
    let mut command = Command::new("npx");
    command.args(["--yes", SERVE_SIM_PACKAGE, "--list", "-q"]);
    if let Some(udid) = udid {
        validate_udid(udid)?;
        command.arg(udid);
    }
    parse_list_output(&run("serve-sim", command)?)
}

/// `npx --yes serve-sim@0.1.45 --kill <udid>`; scoped to the owned stream.
pub fn kill_blocking(udid: &str) -> Result<(), SimStreamError> {
    validate_udid(udid)?;
    let mut command = Command::new("npx");
    command.args(["--yes", SERVE_SIM_PACKAGE, "--kill", udid]);
    run("serve-sim", command).map(|_| ())
}

/// `xcrun simctl shutdown <udid>`; already-shutdown is success.
pub fn shutdown_device_blocking(udid: &str) -> Result<(), SimStreamError> {
    validate_udid(udid)?;
    let mut command = Command::new("xcrun");
    command.args(["simctl", "shutdown", udid]);
    match run("simctl", command) {
        Ok(_) => Ok(()),
        Err(SimStreamError::CommandFailed { detail, .. })
            if is_already_shutdown(&detail) =>
        {
            Ok(())
        }
        Err(error) => Err(error),
    }
}

/// `xcrun simctl ui <udid> appearance <dark|light>` — flips the simulator
/// without touching the running stream helper. Instant; frames keep flowing.
pub fn set_appearance_blocking(udid: &str, dark: bool) -> Result<(), SimStreamError> {
    validate_udid(udid)?;
    let mut command = Command::new("xcrun");
    command.args([
        "simctl",
        "ui",
        udid,
        "appearance",
        if dark { "dark" } else { "light" },
    ]);
    run("simctl", command).map(|_| ())
}

/// Page background for the stripped stream: the app's own surface, by
/// polarity. The serve-sim preview is hardcoded dark with no theme switch,
/// so we paint the letterbox ourselves to match the surrounding panel
/// (`Theme::dark().surface` / `Theme::light().surface`).
pub fn kiosk_page_background(dark: bool) -> &'static str {
    if dark { "#1A1A1A" } else { "#F6F5F6" }
}

/// Script that reduces the serve-sim preview to just the simulator stream.
///
/// The old version searched for a `video`/mjpeg node and walked chrome nodes
/// up by hand — but the default H.264 path renders into a `canvas`, so the
/// lookup missed and nothing was ever stripped (top device pill, bottom
/// home/screenshot toolbar, side rails, brand link all survived React
/// re-renders). This version injects one idempotent `!important` stylesheet
/// instead, so it applies unconditionally — before the stream loads, across
/// re-renders and theme flips — and hides every chrome selector serve-sim
/// renders: all `[data-simulator-toolbar]` bars, the device sidebar toggle +
/// brand link, the right tools rail, side panels, and resize separators. The
/// stream itself (`SimulatorView`, canvas/img/video) is never selected, so
/// video and the input overlay keep working.
pub fn kiosk_script(dark: bool) -> String {
    kiosk_script_with_background(kiosk_page_background(dark), dark)
}

/// Same as [`kiosk_script`], but paints the letterbox with an explicit
/// background (the active app theme's surface hex) instead of the
/// polarity default, so the page tracks custom themes too.
pub fn kiosk_script_with_background(bg: &str, dark: bool) -> String {
    const TEMPLATE: &str = r#"(()=>{try{
const BG='{BG}';
const SCHEME='{SCHEME}';
const ID='insulator-sim-kiosk';
const CSS='html,body{background:'+BG+'!important;margin:0!important;padding:0!important;}'
+':root{color-scheme:'+SCHEME+';--serve-sim-panel-bg:'+BG+';--color-page:'+BG+';}'
+'.bg-page{background:'+BG+'!important;}'
+'div.h-screen{padding:0!important;gap:0!important;}'
+'[data-simulator-toolbar],[data-testid="stream-status-pill"],'
+'a[href*="serve-sim" i],a[href*="evanbacon" i],'
+'button[aria-label="Open tools panel"],button[aria-label="Open WebKit DevTools"],button[aria-label="Open devices sidebar"],'
+'div:has(>button[aria-label="Open tools panel"]),div:has(>button[aria-label="Open devices sidebar"]),'
+'aside,div[role="separator"]{display:none!important;}';
let st=document.getElementById(ID);
if(!st){st=document.createElement('style');st.id=ID;document.head.appendChild(st);}
st.textContent=CSS;
if(document.body)document.body.style.background=BG;
const hideFixed=(btn)=>{let n=btn;while(n&&n!==document.body){const p=n.parentElement;if(!p)break;const c=p.className;if(typeof c==='string'&&c.indexOf('fixed')>=0){p.style.display='none';break;}n=p;}};
document.querySelectorAll('button[aria-label="Open tools panel"],button[aria-label="Open WebKit DevTools"],button[aria-label="Open devices sidebar"]').forEach(hideFixed);
return true;}catch(e){return false;}})()"#;
    TEMPLATE
        .replace("{BG}", bg)
        .replace("{SCHEME}", if dark { "dark" } else { "light" })
}
/// Full stop: kill every stream helper and shut down every booted
/// simulator, best-effort. This is what the Stop button promises — it must
/// work even when the app restarted and lost its owned-stream state, so it
/// deliberately scopes wider than the single owned UDID.
///
/// The graceful pass (`--kill` + `simctl shutdown`) is verified: helpers can
/// outlive `--kill`, so anything still listed is reaped with `SIGKILL` and
/// the shutdown re-issued on a bounded poll. Whatever survives the budget is
/// reported instead of silently leaking.
pub fn stop_all_blocking() -> Result<(), SimStreamError> {
    let mut first_error: Option<SimStreamError> = None;
    let mut note = |error: SimStreamError| {
        if first_error.is_none() {
            first_error = Some(error);
        }
    };

    // Graceful pass.
    match list_streams_blocking(None) {
        Ok(streams) => {
            for stream in streams {
                if let Err(error) = kill_blocking(&stream.device) {
                    note(error);
                }
            }
        }
        Err(error) => note(error),
    }
    match list_devices_blocking() {
        Ok(devices) => {
            for device in devices.iter().filter(|d| d.is_booted()) {
                if let Err(error) = shutdown_device_blocking(&device.udid) {
                    note(error);
                }
            }
        }
        Err(error) => note(error),
    }

    // Verify the stop actually landed; escalate while the budget holds.
    // `simctl shutdown` reports success before the device leaves Booted, so
    // the first poll is expected to still see devices.
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        // A failed verification query must not read as "nothing running":
        // treat it as unverified (keep polling) and preserve the error so
        // Stop cannot report success while helpers/sims are unaccounted for.
        let mut unverified = false;
        let streams: Vec<SimStreamInfo> = match list_streams_blocking(None) {
            Ok(streams) => streams,
            Err(error) => {
                note(error);
                unverified = true;
                Vec::new()
            }
        };
        let booted: Vec<SimDevice> = match list_devices_blocking() {
            Ok(devices) => devices.into_iter().filter(|d| d.is_booted()).collect(),
            Err(error) => {
                note(error);
                unverified = true;
                Vec::new()
            }
        };
        if !unverified && streams.is_empty() && booted.is_empty() {
            break;
        }
        for stream in &streams {
            // Freshly listed, so the pid was alive milliseconds ago; SIGKILL
            // is the escalation when `--kill` left it behind.
            if let Some(pid) = stream.pid {
                force_kill(pid);
            }
            if let Err(error) = kill_blocking(&stream.device) {
                note(error);
            }
        }
        for device in &booted {
            if let Err(error) = shutdown_device_blocking(&device.udid) {
                note(error);
            }
        }
        if Instant::now() >= deadline {
            let mut unverified = false;
            let streams: Vec<SimStreamInfo> = match list_streams_blocking(None) {
                Ok(streams) => streams,
                Err(error) => {
                    note(error);
                    unverified = true;
                    Vec::new()
                }
            };
            let booted: Vec<SimDevice> = match list_devices_blocking() {
                Ok(devices) => devices.into_iter().filter(|d| d.is_booted()).collect(),
                Err(error) => {
                    note(error);
                    unverified = true;
                    Vec::new()
                }
            };
            if unverified && streams.is_empty() && booted.is_empty() {
                return Err(first_error.clone().unwrap_or(SimStreamError::CommandFailed {
                    tool: "serve-sim",
                    detail: "could not verify Stop; retry Stop".to_owned(),
                }));
            }
            if let Some(detail) = describe_remaining(&streams, &booted) {
                return Err(SimStreamError::CommandFailed {
                    tool: "serve-sim",
                    detail,
                });
            }
            break;
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    first_error.map_or(Ok(()), Err)
}

/// Human-readable remainder for a stop that did not fully land, or `None`
/// when nothing is left running.
fn describe_remaining(streams: &[SimStreamInfo], booted: &[SimDevice]) -> Option<String> {
    if streams.is_empty() && booted.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !streams.is_empty() {
        parts.push(format!(
            "{} stream{} still running ({})",
            streams.len(),
            if streams.len() == 1 { "" } else { "s" },
            streams
                .iter()
                .map(|s| s.device.clone())
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    if !booted.is_empty() {
        parts.push(format!(
            "{} simulator{} still booted ({})",
            booted.len(),
            if booted.len() == 1 { "" } else { "s" },
            booted
                .iter()
                .map(|d| d.name.clone())
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    let mut detail = parts.join("; ");
    detail.push_str("; retry Stop or run `npx serve-sim --kill`");
    truncate_to_char_boundary(&mut detail, 300);
    Some(detail)
}

/// Best-effort `SIGKILL`; a stale pid is fine (already gone is the goal).
fn force_kill(pid: u32) {
    let _ = Command::new("kill")
        .args(["-9", &pid.to_string()])
        .output();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn udid_validation_accepts_canonical_form() {
        assert!(validate_udid("31406148-6A0B-49E1-9CFA-4EDAB4D95F9A").is_ok());
        assert!(validate_udid("not-a-udid").is_err());
        assert!(validate_udid("31406148-6A0B-49E1-9CFA-4EDAB4D95F9A; rm -rf /").is_err());
        assert!(validate_udid("").is_err());
    }

    #[test]
    fn simctl_parsing_keeps_runtimes_sorted() {
        let json = r#"{"devices":{"com.apple.CoreSimulator.SimRuntime.iOS-27-0":[{"udid":"B","name":"iPhone B","state":"Shutdown","isAvailable":true},{"udid":"A","name":"iPhone A","state":"Booted","isAvailable":true}]}}"#;
        let devices = parse_simctl_devices(json).expect("parse");
        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "iPhone A");
        assert!(devices[0].is_booted());
    }

    #[test]
    fn preferred_picker_pins_booted_first() {
        let devices = vec![
            SimDevice {
                udid: "BFB704D7-B6FF-4B26-8716-E74298524C05".to_owned(),
                name: "iPad (A16)".to_owned(),
                state: "Booted".to_owned(),
                is_available: true,
            },
            SimDevice {
                udid: "4F17F02D-18C4-4308-91D2-2974CE7A03D0".to_owned(),
                name: "iPhone 17".to_owned(),
                state: "Booted".to_owned(),
                is_available: true,
            },
            SimDevice {
                udid: "31406148-6A0B-49E1-9CFA-4EDAB4D95F9A".to_owned(),
                name: "iPhone 18 Pro".to_owned(),
                state: "Shutdown".to_owned(),
                is_available: true,
            },
        ];
        // Default iPhone wins even over booted devices, and iPads never win.
        assert_eq!(
            pick_preferred(&devices).map(|d| d.name),
            Some("iPhone 18 Pro".to_owned())
        );
        assert!(pick_preferred(&[]).is_none());
    }

    #[test]
    fn preferred_picker_prefers_iphones_without_default() {
        let devices = vec![
            SimDevice {
                udid: "BFB704D7-B6FF-4B26-8716-E74298524C05".to_owned(),
                name: "iPad (A16)".to_owned(),
                state: "Booted".to_owned(),
                is_available: true,
            },
            SimDevice {
                udid: "DEDE129E-7F3D-405E-8FB6-CBE15439CCEB".to_owned(),
                name: "iPhone 17".to_owned(),
                state: "Shutdown".to_owned(),
                is_available: true,
            },
        ];
        assert_eq!(
            pick_preferred(&devices).map(|d| d.name),
            Some("iPhone 17".to_owned())
        );
    }

    #[test]
    fn kiosk_script_targets_chrome_and_theme() {
        let dark = kiosk_script(true);
        assert!(dark.contains("#1A1A1A"));
        assert!(dark.contains("color-scheme:"));
        assert!(dark.contains("dark"));
        assert!(dark.contains("data-simulator-toolbar"));
        assert!(dark.contains("stream-status-pill"));
        assert!(dark.contains("serve-sim"));
        assert!(dark.contains("Open tools panel"));
        assert!(dark.contains("Open devices sidebar"));
        assert!(dark.contains("insulator-sim-kiosk"));
        // Never gates on the stream node: the H.264 path renders into a
        // canvas, and gating left every toolbar visible.
        assert!(!dark.contains("querySelector('video')"));
        let light = kiosk_script(false);
        assert!(light.contains("#F6F5F6"));
        assert!(light.contains("light"));
        let custom = kiosk_script_with_background("#24273a", true);
        assert!(custom.contains("#24273a"));
    }

    #[test]
    fn detach_output_parses_helper_url() {
        let json = r#"{"url":"http://127.0.0.1:3100","streamUrl":"http://127.0.0.1:3100/helper/D/stream.mjpeg","wsUrl":"ws://127.0.0.1:3100/helper/D/ws","port":3100,"device":"31406148-6A0B-49E1-9CFA-4EDAB4D95F9A"}"#;
        let info = parse_detach_output(json).expect("parse");
        assert_eq!(info.url, "http://127.0.0.1:3100");
        assert_eq!(info.port, 3100);
    }

    #[test]
    fn list_output_handles_empty_and_array() {
        assert!(parse_list_output("").expect("empty").is_empty());
        assert!(parse_list_output("[]").expect("array").is_empty());
    }

    fn test_stream(device: &str) -> SimStreamInfo {
        SimStreamInfo {
            device: device.to_owned(),
            url: "http://127.0.0.1:3100".to_owned(),
            stream_url: String::new(),
            ws_url: String::new(),
            port: 3100,
            pid: Some(1234),
        }
    }

    fn test_device(name: &str) -> SimDevice {
        SimDevice {
            udid: "31406148-6A0B-49E1-9CFA-4EDAB4D95F9A".to_owned(),
            name: name.to_owned(),
            state: "Booted".to_owned(),
            is_available: true,
        }
    }

    #[test]
    fn detach_output_rejects_off_loopback_urls() {
        let cases = [
            // Non-loopback host: must not be stored — the preview page
            // exposes a shell-execution route.
            r#"{"url":"http://192.168.1.5:3100","device":"31406148-6A0B-49E1-9CFA-4EDAB4D95F9A"}"#,
            r#"{"url":"http://example.com:3100","device":"31406148-6A0B-49E1-9CFA-4EDAB4D95F9A"}"#,
            // Non-HTTP scheme.
            r#"{"url":"file:///tmp/evil.html","device":"31406148-6A0B-49E1-9CFA-4EDAB4D95F9A"}"#,
            r#"{"url":"https://127.0.0.1:3100","device":"31406148-6A0B-49E1-9CFA-4EDAB4D95F9A"}"#,
            // Wrong-scheme companion URL.
            r#"{"url":"http://127.0.0.1:3100","streamUrl":"http://127.0.0.1:3100/x","wsUrl":"http://127.0.0.1:3100/ws","device":"31406148-6A0B-49E1-9CFA-4EDAB4D95F9A"}"#,
        ];
        for json in cases {
            assert!(parse_detach_output(json).is_err(), "accepted: {json}");
        }
    }

    #[test]
    fn truncation_stops_at_char_boundary() {
        let mut detail = "é".repeat(400);
        truncate_to_char_boundary(&mut detail, 300);
        assert!(detail.len() <= 300);
        assert!(detail.is_char_boundary(detail.len()));
        assert!(!detail.is_empty());
        let mut short = String::from("ok");
        truncate_to_char_boundary(&mut short, 300);
        assert_eq!(short, "ok");
    }

    #[test]
    fn stop_remainder_with_non_ascii_name_does_not_panic() {
        let streams = vec![test_stream("D1")];
        let booted = vec![test_device(&"é".repeat(400))];
        let detail = describe_remaining(&streams, &booted).expect("remainder");
        assert!(detail.len() <= 400);
        assert!(detail.is_char_boundary(detail.len()));
    }

    #[test]
    fn boot_guard_matches_real_simctl_state_suffix() {
        assert!(is_already_booted(
            "Unable to boot device in current state: Booted"
        ));
        assert!(is_already_booted("Already booted"));
        assert!(!is_already_booted(
            "Unable to boot device in current state: Shutdown"
        ));
        assert!(!is_already_booted("Invalid device state"));
        assert!(is_already_shutdown(
            "Unable to shutdown device in current state: Shutdown"
        ));
        assert!(!is_already_shutdown(
            "Unable to shutdown device in current state: Booted"
        ));
    }
    #[test]
    fn stop_remainder_reports_what_survived() {
        assert!(describe_remaining(&[], &[]).is_none());
        let streams = vec![test_stream("D1")];
        let booted = vec![test_device("iPhone 18 Pro")];
        let detail = describe_remaining(&streams, &booted).expect("remainder");
        assert!(detail.contains("1 stream still running"));
        assert!(detail.contains("1 simulator still booted"));
        assert!(detail.contains("iPhone 18 Pro"));
        let streams_only = describe_remaining(&streams, &[]).expect("remainder");
        assert!(streams_only.contains("stream"));
        assert!(!streams_only.contains("booted"));
    }
}
