//! Terminal mode: a full-window workspace of shell tabs that replaces the
//! agent UI until the user switches back.
//!
//! The two modes never share a session. Agents keep running while the
//! terminal is up, and `note_agent_attention` surfaces a finished turn or a
//! pending question as a badge on the switcher plus a toast.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::*;

const TERMINAL_TITLEBAR_HEIGHT: f32 = 48.0;
const TERMINAL_FRAME_PADDING_X: f32 = 10.0;
const TERMINAL_FRAME_PADDING_TOP: f32 = 2.0;
const TERMINAL_FRAME_PADDING_BOTTOM: f32 = 10.0;
const TERMINAL_FRAME_BORDER_WIDTH: f32 = 1.0;
const TERMINAL_FRAME_RADIUS: f32 = 10.0;
const TERMINAL_AGENT_POLL_INTERVAL: Duration = Duration::from_millis(150);

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum AppMode {
    #[default]
    Agents,
    Terminal,
}

pub(super) struct TerminalTab {
    id: Uuid,
    view: Entity<TerminalView>,
    /// CLI agent found running under the shell, refreshed by a poll.
    agent: Option<TerminalAgent>,
}

/// What survives a quit. Shell processes do not; a restored tab starts a
/// fresh shell in `cwd` under a dimmed replay of `history`.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(super) struct SavedTerminalLayout {
    #[serde(default)]
    pub(super) mode: AppMode,
    #[serde(default)]
    active: usize,
    #[serde(default)]
    tabs: Vec<SavedTerminalTab>,
    /// Zoomed terminal font size; absent means the default.
    #[serde(default)]
    pub(super) font_size: Option<f32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct SavedTerminalTab {
    cwd: PathBuf,
    #[serde(default)]
    history: String,
    /// A program from `RESUMABLE_PROGRAMS` that was running in the tab at
    /// quit. It is started again in the restored shell.
    #[serde(default)]
    resume: Option<String>,
    /// Shell process id, only to find `resume` at write time.
    #[serde(skip)]
    child_pid: Option<u32>,
}

/// Programs that keep their own session state, so running them again in a
/// fresh shell reattaches instead of starting over. Also the only commands
/// a saved layout may ask the app to run.
const RESUMABLE_PROGRAMS: &[&str] = &["herdr"];

fn resumable_program(name: &str) -> Option<&'static str> {
    RESUMABLE_PROGRAMS
        .iter()
        .copied()
        .find(|program| *program == name)
}

/// A CLI coding agent running in a terminal tab. Its mark replaces the
/// generic terminal icon on the tab.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TerminalAgent {
    Claude,
    Codex,
    OpenCode,
    Pi,
    Cline,
    Copilot,
    Gemini,
    Cursor,
    Amp,
    Antigravity,
}

impl TerminalAgent {
    /// Matches an executable or script name, e.g. `claude` or `gemini.js`.
    fn from_program(name: &str) -> Option<Self> {
        let stem = name.split('.').next().unwrap_or(name);
        match stem {
            "claude" => Some(Self::Claude),
            "codex" => Some(Self::Codex),
            "opencode" => Some(Self::OpenCode),
            "pi" => Some(Self::Pi),
            "cline" => Some(Self::Cline),
            "copilot" => Some(Self::Copilot),
            "gemini" => Some(Self::Gemini),
            "cursor-agent" => Some(Self::Cursor),
            "amp" => Some(Self::Amp),
            "agy" => Some(Self::Antigravity),
            _ => None,
        }
    }

    /// Matches a full command line. Node-based agents run as
    /// `node /path/to/gemini`, so the script name counts as well as the
    /// executable. Claude's native install runs from a `claude/versions/`
    /// directory, so its path components count too.
    fn from_command(command: &str) -> Option<Self> {
        let mut words = command.split_whitespace();
        let program = words.next()?;
        let file_name = |word: &str| {
            Path::new(word)
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(Self::from_program)
        };
        file_name(program)
            .or_else(|| words.next().and_then(file_name))
            .or_else(|| {
                Path::new(program)
                    .components()
                    .any(|part| part.as_os_str() == "claude")
                    .then_some(Self::Claude)
            })
    }

    fn icon(self) -> TerminalAgentIcon {
        match self {
            Self::Claude => TerminalAgentIcon::Color("icons/agent-claude.svg"),
            Self::Codex => TerminalAgentIcon::Mono("icons/provider-codex.svg"),
            Self::Gemini => TerminalAgentIcon::Color("icons/agent-gemini.svg"),
            Self::Antigravity => TerminalAgentIcon::Color("icons/agent-antigravity.svg"),
            Self::OpenCode => TerminalAgentIcon::Mono("icons/provider-opencode.svg"),
            Self::Pi => TerminalAgentIcon::Mono("icons/provider-pi.svg"),
            Self::Cline => TerminalAgentIcon::Mono("icons/provider-cline.svg"),
            Self::Copilot => TerminalAgentIcon::Mono("icons/provider-copilot.svg"),
            Self::Cursor => TerminalAgentIcon::Mono("icons/provider-cursor.svg"),
            Self::Amp => TerminalAgentIcon::Mono("icons/provider-amp.svg"),
        }
    }
}

/// Brand-coloured marks render as images. Single-colour marks render as a
/// mask tinted with the tab's text colour, since black would vanish on a dark
/// theme.
enum TerminalAgentIcon {
    Color(&'static str),
    Mono(&'static str),
}

/// For each shell pid, the first program `pick` accepts among the commands
/// running anywhere under it. One `ps` call covers every pid. Blocking, so
/// callers run it off the UI thread.
#[cfg(unix)]
fn programs_under<T: Copy>(roots: &[u32], pick: impl Fn(&str) -> Option<T>) -> HashMap<u32, T> {
    let Ok(output) = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,command="])
        .output()
    else {
        return HashMap::new();
    };
    let listing = String::from_utf8_lossy(&output.stdout);
    let mut children: HashMap<u32, Vec<(u32, &str)>> = HashMap::new();
    for line in listing.lines() {
        let mut fields = line.split_whitespace();
        let (Some(pid), Some(parent)) = (fields.next(), fields.next()) else {
            continue;
        };
        let (Ok(pid), Ok(parent)) = (pid.parse::<u32>(), parent.parse::<u32>()) else {
            continue;
        };
        let command = line
            .trim_start()
            .splitn(3, char::is_whitespace)
            .nth(2)
            .map_or("", str::trim);
        children.entry(parent).or_default().push((pid, command));
    }
    let mut found = HashMap::new();
    for root in roots {
        let mut pending = vec![*root];
        while let Some(pid) = pending.pop() {
            for (child, command) in children.get(&pid).into_iter().flatten() {
                if let Some(program) = pick(command) {
                    found.insert(*root, program);
                }
                pending.push(*child);
            }
        }
    }
    found
}

#[cfg(not(unix))]
fn programs_under<T: Copy>(_roots: &[u32], _pick: impl Fn(&str) -> Option<T>) -> HashMap<u32, T> {
    HashMap::new()
}

fn resumable_programs_under(roots: &[u32]) -> HashMap<u32, &'static str> {
    programs_under(roots, |command| {
        let name = command
            .split_whitespace()
            .next()
            .and_then(|program| Path::new(program).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        resumable_program(name)
    })
}

/// The agent a process is, by its command line. Blocking: runs `ps`.
#[cfg(unix)]
fn agent_running_as(pid: u32) -> Option<TerminalAgent> {
    let output = std::process::Command::new("ps")
        .args(["-o", "command=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    TerminalAgent::from_command(String::from_utf8_lossy(&output.stdout).trim())
}

#[cfg(not(unix))]
fn agent_running_as(_pid: u32) -> Option<TerminalAgent> {
    None
}

/// An agent event worth a toast while the terminal is on screen.
#[derive(Clone, Copy)]
pub(super) enum AgentAttention {
    TurnFinished { success: bool },
    NeedsInput,
}

fn terminal_layout_path() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("Insulator")
        .join("terminal-layout.json")
}

impl SavedTerminalLayout {
    /// Read at startup. A missing or unreadable file is a first run, not an
    /// error: the app opens in Agents mode.
    pub(super) fn load() -> Self {
        std::fs::read(terminal_layout_path())
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Blocking: looks up running processes and writes the file. Callers run
    /// it on a background thread.
    pub(super) fn write(&self) {
        let path = terminal_layout_path();
        let Some(parent) = path.parent() else { return };
        let _ = std::fs::create_dir_all(parent);
        let mut layout = self.clone();
        layout.record_resumable_programs();
        let Ok(bytes) = serde_json::to_vec(&layout) else {
            return;
        };
        let _ = std::fs::write(path, bytes);
    }

    /// Marks tabs that have a resumable program running. Their saved output
    /// is that program's own screen, which it redraws, so it is dropped.
    fn record_resumable_programs(&mut self) {
        let roots = self
            .tabs
            .iter()
            .filter_map(|tab| tab.child_pid)
            .collect::<Vec<_>>();
        if roots.is_empty() {
            return;
        }
        let found = resumable_programs_under(&roots);
        for tab in &mut self.tabs {
            let Some(program) = tab.child_pid.and_then(|pid| found.get(&pid)) else {
                // A live shell with nothing resumable under it. Tabs with no
                // pid are saved layout carried through unopened; they keep
                // their mark.
                if tab.child_pid.is_some() {
                    tab.resume = None;
                }
                continue;
            };
            tab.resume = Some((*program).to_owned());
            tab.history.clear();
        }
    }
}

impl Insulator {
    pub(super) fn set_app_mode(&mut self, mode: AppMode, cx: &mut Context<Self>) {
        if self.app_mode == mode {
            return;
        }
        self.app_mode = mode;
        self.terminal_mode_switch_generation = self.terminal_mode_switch_generation.wrapping_add(1);
        match mode {
            AppMode::Terminal => {
                self.ensure_terminal_tabs(cx);
                self.terminal_pending_focus = true;
            }
            AppMode::Agents => {
                // Land on the session that asked for attention, newest first.
                let target = self.terminal_attention.last().copied();
                self.terminal_attention.clear();
                if let Some(session_id) = target {
                    self.select_session(session_id, cx);
                }
                self.agents_pending_focus = true;
            }
        }
        self.save_terminal_layout(cx);
        cx.notify();
    }

    pub(super) fn switch_to_terminal_mode_action(
        &mut self,
        _: &SwitchToTerminalMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_app_mode(AppMode::Terminal, cx);
    }

    pub(super) fn switch_to_agents_mode_action(
        &mut self,
        _: &SwitchToAgentsMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_app_mode(AppMode::Agents, cx);
    }

    fn select_terminal_tab_action(
        &mut self,
        action: &SelectTerminalTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(last) = self.terminal_tabs.len().checked_sub(1) else {
            return;
        };
        self.select_terminal_tab(action.0.min(last), cx);
    }

    /// Called each agent-window frame. Moves focus to the composer after a
    /// switch back from the terminal, and otherwise makes sure something in
    /// the window holds focus, since shortcuts only reach handlers on the path
    /// from the focused element.
    pub(super) fn restore_agents_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pending = std::mem::take(&mut self.agents_pending_focus);
        if pending && self.selected_session().is_some() {
            let composer = self.composer_focus(cx);
            window.focus(&composer, cx);
        } else if pending || window.focused(cx).is_none() {
            window.focus(&self.app_focus, cx);
        }
    }

    pub(super) fn toggle_terminal_mode_action(
        &mut self,
        _: &ToggleTerminalMode,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let next = match self.app_mode {
            AppMode::Agents => AppMode::Terminal,
            AppMode::Terminal => AppMode::Agents,
        };
        self.set_app_mode(next, cx);
    }

    /// Opens the saved tabs on first use, or one fresh tab when there is
    /// nothing to restore. Spawning is lazy so users who never open the
    /// terminal never pay for a shell.
    fn ensure_terminal_tabs(&mut self, cx: &mut Context<Self>) {
        if !self.terminal_tabs.is_empty() {
            return;
        }
        if let Some(saved) = self.terminal_pending_restore.take() {
            for tab in saved.tabs {
                let cwd = if tab.cwd.is_dir() {
                    tab.cwd
                } else {
                    self.default_terminal_cwd()
                };
                let restore = TerminalRestore {
                    history: (!tab.history.is_empty()).then_some(tab.history),
                    // Only allowlisted names run, whatever the file says.
                    startup_command: tab
                        .resume
                        .as_deref()
                        .and_then(resumable_program)
                        .map(str::to_owned),
                };
                self.push_terminal_tab(cwd, restore, cx);
            }
            self.active_terminal_tab = saved.active.min(self.terminal_tabs.len().saturating_sub(1));
        }
        if self.terminal_tabs.is_empty() {
            let cwd = self.default_terminal_cwd();
            self.push_terminal_tab(cwd, TerminalRestore::default(), cx);
            self.active_terminal_tab = 0;
        }
    }

    /// The active project's root, falling back to the home directory. A
    /// remote daemon's paths belong to another machine, so they are skipped.
    fn default_terminal_cwd(&self) -> PathBuf {
        let workspace = (!self.daemon.is_remote())
            .then(|| self.selected_workspace_path().map(Path::to_path_buf))
            .flatten()
            .filter(|path| path.is_dir());
        workspace
            .or_else(|| self.home_directory.clone())
            .or_else(dirs::home_dir)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("/"))
    }

    fn push_terminal_tab(
        &mut self,
        cwd: PathBuf,
        restore: TerminalRestore,
        cx: &mut Context<Self>,
    ) {
        let view = cx.new(|cx| TerminalView::restored(cwd, restore, cx));
        // Redraw the title bar when the shell retitles itself.
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        // `exit` in the shell closes its tab; the last tab leaving ends terminal
        // mode (see `close_terminal_tab`).
        cx.subscribe(
            &view,
            |this: &mut Self, view: Entity<TerminalView>, _: &TerminalExited, cx| {
                if let Some(index) = this.terminal_tabs.iter().position(|tab| tab.view == view) {
                    this.close_terminal_tab(index, cx);
                }
            },
        )
        .detach();
        let id = Uuid::new_v4();
        self.terminal_tabs.push(TerminalTab {
            id,
            view,
            agent: None,
        });
        self.watch_terminal_agent(id, cx);
    }

    /// Watches which program owns the tab's terminal so its icon follows the
    /// agent running in it. The foreground check is a syscall, so it runs
    /// often; the `ps` lookup runs only when the foreground program changes.
    /// Stops once the tab is closed. Idle while Agents mode is up.
    fn watch_terminal_agent(&self, tab_id: Uuid, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let mut last_foreground = None;
            loop {
                cx.background_executor()
                    .timer(TERMINAL_AGENT_POLL_INTERVAL)
                    .await;
                let Ok(watch) = this.update(cx, |this, cx| {
                    let tab = this.terminal_tabs.iter().find(|tab| tab.id == tab_id);
                    tab.map(|tab| {
                        let view = tab.view.read(cx);
                        (this.app_mode == AppMode::Terminal)
                            .then(|| (view.child_pid(), view.foreground_pgid()))
                    })
                }) else {
                    return;
                };
                let Some(watch) = watch else { return };
                let Some((Some(shell), Some(foreground))) = watch else {
                    continue;
                };
                if last_foreground == Some(foreground) {
                    continue;
                }
                last_foreground = Some(foreground);
                let agent = if foreground == shell {
                    None
                } else {
                    cx.background_executor()
                        .spawn(async move { agent_running_as(foreground) })
                        .await
                };
                let Ok(()) = this.update(cx, |this, cx| {
                    if let Some(tab) = this.terminal_tabs.iter_mut().find(|tab| tab.id == tab_id) {
                        if tab.agent != agent {
                            tab.agent = agent;
                            cx.notify();
                        }
                    }
                }) else {
                    return;
                };
            }
        })
        .detach();
    }

    pub(super) fn new_terminal_tab(&mut self, cx: &mut Context<Self>) {
        let cwd = self.default_terminal_cwd();
        self.push_terminal_tab(cwd, TerminalRestore::default(), cx);
        self.active_terminal_tab = self.terminal_tabs.len() - 1;
        self.terminal_pending_focus = true;
        self.save_terminal_layout(cx);
        cx.notify();
    }

    fn select_terminal_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.terminal_tabs.len() || index == self.active_terminal_tab {
            return;
        }
        self.active_terminal_tab = index;
        self.terminal_pending_focus = true;
        cx.notify();
    }

    pub(super) fn cycle_terminal_tabs(&mut self, backwards: bool, cx: &mut Context<Self>) {
        let count = self.terminal_tabs.len();
        if count < 2 {
            return;
        }
        let next = if backwards {
            (self.active_terminal_tab + count - 1) % count
        } else {
            (self.active_terminal_tab + 1) % count
        };
        self.select_terminal_tab(next, cx);
    }

    /// Closing the last tab leaves terminal mode; the next entry opens a
    /// fresh shell.
    pub(super) fn close_terminal_tab(&mut self, index: usize, cx: &mut Context<Self>) {
        if index >= self.terminal_tabs.len() {
            return;
        }
        self.terminal_tabs.remove(index);
        if self.terminal_tabs.is_empty() {
            self.active_terminal_tab = 0;
            self.set_app_mode(AppMode::Agents, cx);
            return;
        }
        if self.active_terminal_tab > index || self.active_terminal_tab >= self.terminal_tabs.len()
        {
            self.active_terminal_tab = self.active_terminal_tab.saturating_sub(1);
        }
        self.terminal_pending_focus = true;
        self.save_terminal_layout(cx);
        cx.notify();
    }

    pub(super) fn close_active_terminal_tab(&mut self, cx: &mut Context<Self>) {
        self.close_terminal_tab(self.active_terminal_tab, cx);
    }

    /// Snapshot for disk. Tabs that were never opened keep the saved layout
    /// so a session spent entirely in Agents mode does not erase it.
    pub(super) fn terminal_layout_snapshot(&self, cx: &App) -> SavedTerminalLayout {
        if self.terminal_tabs.is_empty() {
            let mut layout = self.terminal_pending_restore.clone().unwrap_or_default();
            layout.mode = self.app_mode;
            layout.font_size = Some(crate::terminal::terminal_font_size());
            return layout;
        }
        SavedTerminalLayout {
            mode: self.app_mode,
            active: self.active_terminal_tab,
            font_size: Some(crate::terminal::terminal_font_size()),
            tabs: self
                .terminal_tabs
                .iter()
                .map(|tab| {
                    let view = tab.view.read(cx);
                    SavedTerminalTab {
                        cwd: view.working_directory().to_path_buf(),
                        history: view.history_text().unwrap_or_default(),
                        resume: None,
                        child_pid: view.child_pid(),
                    }
                })
                .collect(),
        }
    }

    /// Captures on the UI thread (a grid lock, no I/O), writes on a
    /// background thread.
    pub(super) fn save_terminal_layout(&self, cx: &mut Context<Self>) {
        let layout = self.terminal_layout_snapshot(cx);
        cx.background_executor()
            .spawn(async move { layout.write() })
            .detach();
    }

    /// Records a finished turn or a pending question while the terminal is
    /// on screen. Does nothing in Agents mode, where the session list
    /// already shows it.
    pub(super) fn note_agent_attention(&mut self, session_id: Uuid, kind: AgentAttention) {
        if self.app_mode != AppMode::Terminal {
            return;
        }
        let title = self
            .state
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .map(|session| {
                if session.display_title() == AgentSession::DEFAULT_TITLE {
                    tr!("session.new_task")
                } else {
                    session.display_title().to_owned()
                }
            })
            .unwrap_or_else(|| tr!("session.new_task"));
        self.terminal_attention.retain(|id| *id != session_id);
        self.terminal_attention.push(session_id);
        match kind {
            AgentAttention::TurnFinished { success: true } => {
                self.show_success_toast(format!("{title}: {}", tr!("session.turn_completed")));
            }
            AgentAttention::TurnFinished { success: false } => {
                self.show_toast(format!("{title}: {}", tr!("session.stopped")));
            }
            AgentAttention::NeedsInput => {
                self.show_toast(format!("{title}: {}", tr!("terminal.needs_input")));
            }
        }
    }

    /// The two-segment switcher shown beside the resource button. Always
    /// available, whether or not the resource button is enabled.
    pub(super) fn render_mode_switcher(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::current(cx);
        let solid = self.state.window_style == WindowStyle::Solid;
        let attention = self.terminal_attention.len();
        let is_terminal = self.app_mode == AppMode::Terminal;
        let generation = self.terminal_mode_switch_generation;

        let pill: AnyElement = if generation == 0 {
            div()
                .absolute()
                .top(px(2.0))
                .left(if is_terminal { px(32.0) } else { px(2.0) })
                .w(px(28.0))
                .h(px(22.0))
                .rounded(px(5.0))
                .bg(theme.raised)
                .when(solid, |element| element.shadow_xs())
                .into_any_element()
        } else {
            let (from_x, to_x) = if is_terminal {
                (2.0, 32.0)
            } else {
                (32.0, 2.0)
            };
            div()
                .absolute()
                .top(px(2.0))
                .w(px(28.0))
                .h(px(22.0))
                .rounded(px(5.0))
                .bg(theme.raised)
                .when(solid, |element| element.shadow_xs())
                .with_animation(
                    SharedString::from(format!("mode-switcher-pill-{generation}")),
                    Animation::new(Duration::from_millis(180)).with_easing(ease_out_quint()),
                    move |element, delta| {
                        let x = from_x + (to_x - from_x) * delta;
                        element.left(px(x))
                    },
                )
                .into_any_element()
        };

        let segment = |id: &'static str,
                       mode: AppMode,
                       icon_path: &'static str,
                       label: String,
                       badge: usize| {
            let selected = self.app_mode == mode;
            div()
                .id(id)
                .tab_index(0)
                .focus_visible(|element| element.border_1().border_color(theme.accent))
                .relative()
                .w(px(28.0))
                .h(px(22.0))
                .flex_none()
                .rounded(px(5.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .when(!selected, |element| {
                    element.hover(|element| element.bg(theme.overlay_strong))
                })
                .tooltip(Tooltip::text(format!(
                    "{} ({}+J)",
                    label,
                    if cfg!(target_os = "macos") {
                        "⌘"
                    } else {
                        "Ctrl"
                    }
                )))
                .child(icon(
                    icon_path,
                    13.0,
                    if selected {
                        theme.text
                    } else {
                        theme.text_tertiary
                    },
                ))
                .when(badge > 0, |element| {
                    element.child(
                        div()
                            .absolute()
                            .top(px(2.0))
                            .right(px(3.0))
                            .size(px(7.0))
                            .rounded_full()
                            .bg(theme.accent)
                            .border_1()
                            .border_color(if selected {
                                theme.raised
                            } else {
                                theme.overlay
                            }),
                    )
                })
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.set_app_mode(mode, cx);
                }))
        };

        div()
            .id("mode-switcher")
            .relative()
            .p(px(2.0))
            .w(px(62.0))
            .h(px(26.0))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.0))
            .rounded(px(7.0))
            .bg(theme.overlay)
            .child(pill)
            .child(segment(
                "mode-agents",
                AppMode::Agents,
                "icons/bot.svg",
                tr!("terminal.mode_agents"),
                attention,
            ))
            .child(segment(
                "mode-terminal",
                AppMode::Terminal,
                "icons/terminal.svg",
                tr!("terminal.mode_terminal"),
                0,
            ))
    }

    fn render_terminal_tabs(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::current(cx);
        let solid = self.state.window_style == WindowStyle::Solid;
        let terminal_icon = crate::assets::terminal_icon();
        let mut strip = div()
            .id("terminal-mode-tabs")
            .h_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.0))
            .overflow_x_scroll();
        for (index, tab) in self.terminal_tabs.iter().enumerate() {
            let selected = index == self.active_terminal_tab;
            let view = tab.view.read(cx);
            // Agents prefix their title with a status glyph (Claude Code's
            // spinner), which would sit next to the tab icon as a second one.
            // Shells title the tab with the working directory; keep only
            // the folder name.
            let title = view.title().trim();
            let title = if title.contains('/') {
                title
                    .rsplit('/')
                    .find(|part| !part.is_empty())
                    .unwrap_or("~")
            } else {
                title
            };
            let title = title
                .trim_start_matches(|c: char| !c.is_alphanumeric() && c != '~')
                .trim();
            let is_generic_shell = matches!(
                title.to_ascii_lowercase().as_str(),
                "" | "zsh" | "bash" | "sh" | "fish"
            );
            let label = if !is_generic_shell {
                title.to_owned()
            } else {
                let cwd = view.working_directory();
                if dirs::home_dir().is_some_and(|home| cwd == home) {
                    "~".to_owned()
                } else {
                    cwd.file_name()
                        .and_then(|name| name.to_str())
                        .map(str::to_owned)
                        .unwrap_or_else(|| format!("{} {}", tr!("right_panel.terminal"), index + 1))
                }
            };
            let exited = view.is_exited();
            let tab_group = SharedString::from(format!("terminal-tab-{}", tab.id));
            let icon_element = div()
                .h(px(20.0))
                .w(px(24.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .child(match tab.agent.map(TerminalAgent::icon) {
                    Some(TerminalAgentIcon::Color(path)) => {
                        file_icon(path, 16.0).into_any_element()
                    }
                    Some(TerminalAgentIcon::Mono(path)) => icon(
                        path,
                        15.0,
                        if selected {
                            theme.text
                        } else {
                            theme.text_tertiary
                        },
                    )
                    .into_any_element(),
                    None => img(terminal_icon.clone())
                        .size_full()
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                })
                .when(!selected && tab.agent.is_none(), |el| el.opacity(0.60));

            strip = strip.child(
                div()
                    .id(tab_group.clone())
                    .group(tab_group.clone())
                    .tab_index(0)
                    .focus_visible(|element| element.border_color(theme.accent))
                    .h(px(32.0))
                    .min_w(px(80.0))
                    .max_w(px(260.0))
                    .pl(px(6.0))
                    .pr(px(14.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded_full()
                    .cursor_pointer()
                    .text_size(sp(13.0))
                    .border_1()
                    .when(selected, |element| {
                        element
                            .bg(theme.raised)
                            .border_color(theme.border_strong)
                            .text_color(theme.text)
                            .font_weight(FontWeight::MEDIUM)
                            .when(solid, |element| element.shadow_xs())
                    })
                    .when(!selected, |element| {
                        element
                            .bg(gpui::transparent_black())
                            .border_color(gpui::transparent_black())
                            .text_color(theme.text_tertiary)
                            .font_weight(FontWeight::NORMAL)
                            .hover(|element| {
                                element
                                    .bg(theme.overlay)
                                    .border_color(theme.border)
                                    .text_color(theme.text_secondary)
                            })
                    })
                    .tooltip(Tooltip::text(format!(
                        "{} ({}+{})",
                        label,
                        if cfg!(target_os = "macos") {
                            "⌘"
                        } else {
                            "Ctrl"
                        },
                        if index < 9 {
                            (index + 1).to_string()
                        } else {
                            "9".to_string()
                        }
                    )))
                    .on_mouse_down(
                        MouseButton::Middle,
                        cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.close_terminal_tab(index, cx);
                        }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_terminal_tab(index, cx);
                    }))
                    .child(icon_element)
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .child(SharedString::from(label)),
                    )
                    .when(exited, |element| {
                        element.child(
                            div()
                                .size(px(6.0))
                                .rounded_full()
                                .flex_none()
                                .bg(theme.danger),
                        )
                    })
                    .child(
                        div()
                            .id(SharedString::from(format!("close-terminal-tab-{}", tab.id)))
                            .size(px(16.0))
                            .flex_none()
                            .rounded_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .opacity(0.0)
                            .group_hover(tab_group, |style| style.opacity(0.65))
                            .hover(|element| {
                                element
                                    .opacity(1.0)
                                    .bg(theme.overlay_strong)
                                    .text_color(theme.text)
                            })
                            .child(icon("icons/x.svg", 9.0, theme.text_secondary))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.close_terminal_tab(index, cx);
                            })),
                    ),
            );
        }
        strip.child(
            div()
                .id("new-terminal-tab")
                .size(px(28.0))
                .flex_none()
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .border_1()
                .border_color(gpui::transparent_black())
                .hover(|element| element.bg(theme.overlay).border_color(theme.border))
                .active(|element| element.bg(theme.overlay_strong))
                .tooltip(Tooltip::text(format!(
                    "{} ({}+T)",
                    tr!("terminal.new_tab"),
                    if cfg!(target_os = "macos") {
                        "⌘"
                    } else {
                        "Ctrl"
                    }
                )))
                .child(icon("icons/plus.svg", 13.0, theme.text_secondary))
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.new_terminal_tab(cx);
                })),
        )
    }

    fn render_terminal_titlebar(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("terminal-mode-titlebar")
            .h(px(TERMINAL_TITLEBAR_HEIGHT))
            .w_full()
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .pr(px(12.0))
            .children(self.render_client_window_controls(
                super::window_chrome::WindowControlSide::Left,
                window,
                cx,
            ))
            .child(
                self.window_drag_region(
                    div()
                        .id("terminal-traffic-light-drag-region")
                        .w(px(TRAFFIC_LIGHT_CLEARANCE))
                        .h_full()
                        .flex_none(),
                    cx,
                ),
            )
            .child(self.render_terminal_tabs(cx))
            .child(self.window_drag_region(
                div().id("terminal-titlebar-drag-region").h_full().flex_1(),
                cx,
            ))
            .child(self.render_mode_switcher(cx))
            .children(self.render_client_window_controls(
                super::window_chrome::WindowControlSide::Right,
                window,
                cx,
            ))
    }

    /// The whole window in terminal mode: title bar, then the active
    /// terminal filling the rest. No sidebar, header, or panels.
    pub(super) fn render_terminal_mode(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.ensure_terminal_tabs(cx);
        let theme = Theme::current(cx);
        let viewport = window.viewport_size();
        let body_width = (f32::from(viewport.width)
            - TERMINAL_FRAME_PADDING_X * 2.0
            - TERMINAL_FRAME_BORDER_WIDTH * 2.0)
            .max(100.0);
        let body_height = (f32::from(viewport.height)
            - TERMINAL_TITLEBAR_HEIGHT
            - TERMINAL_FRAME_PADDING_TOP
            - TERMINAL_FRAME_PADDING_BOTTOM
            - TERMINAL_FRAME_BORDER_WIDTH * 2.0)
            .max(60.0);
        let active = self
            .terminal_tabs
            .get(self.active_terminal_tab)
            .map(|tab| tab.view.clone());
        if let Some(view) = active.as_ref() {
            view.update(cx, |view, _| {
                view.set_show_toolbar(false);
                view.set_panel_size(body_width, body_height);
            });
            if self.terminal_pending_focus {
                let focus_handle = view.read(cx).focus_handle(cx);
                window.focus(&focus_handle, cx);
                self.terminal_pending_focus = false;
            }
        }
        let window_style = self.state.window_style;
        let toast = self.render_active_toast(cx);
        if window.focused(cx).is_none() {
            window.focus(&self.app_focus, cx);
        }
        let root = div()
            .key_context("Insulator TerminalMode")
            .track_focus(&self.app_focus)
            .on_action(cx.listener(Self::toggle_terminal_mode_action))
            .on_action(cx.listener(Self::switch_to_agents_mode_action))
            .on_action(cx.listener(Self::select_terminal_tab_action))
            .on_action(cx.listener(Self::new_tab_action))
            .on_action(cx.listener(Self::close_window_or_right_panel_tab_action))
            .on_action(cx.listener(Self::close_active_tab_action))
            .on_action(cx.listener(Self::next_main_tab_action))
            .on_action(cx.listener(Self::previous_main_tab_action))
            .on_action(cx.listener(Self::open_settings_action))
            .size_full()
            .relative()
            .flex()
            .flex_col()
            .text_color(theme.text)
            .font_family(crate::theme::active_ui_font_family())
            .bg(match window_style {
                WindowStyle::LiquidGlass => Hsla {
                    a: 0.10,
                    ..theme.surface
                },
                // Same translucency as the agent window's content column, with
                // the wallpaper drawn underneath for the Image style.
                WindowStyle::Image => Hsla {
                    a: 0.82,
                    ..theme.surface
                },
                WindowStyle::Solid => theme.surface,
                WindowStyle::Transparent => theme.surface,
            })
            .when_some(
                (window_style == WindowStyle::Image)
                    .then(|| self.state.background_image_path.clone())
                    .flatten(),
                |root, path| {
                    root.child(
                        img(std::path::PathBuf::from(path))
                            .absolute()
                            .inset_0()
                            .size_full()
                            .object_fit(ObjectFit::Cover),
                    )
                },
            )
            .child(self.render_terminal_titlebar(window, cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .px(px(TERMINAL_FRAME_PADDING_X))
                    .pt(px(TERMINAL_FRAME_PADDING_TOP))
                    .pb(px(TERMINAL_FRAME_PADDING_BOTTOM))
                    .child(
                        div()
                            .id("terminal-frame")
                            .size_full()
                            .min_h_0()
                            .min_w_0()
                            .rounded(px(TERMINAL_FRAME_RADIUS))
                            .border_1()
                            .border_color(theme.border_strong)
                            .bg(theme.terminal)
                            .overflow_hidden()
                            .when(window_style == WindowStyle::Solid, |frame| {
                                frame.shadow_md()
                            })
                            .children(active),
                    ),
            )
            .children(toast);

        let generation = self.terminal_mode_switch_generation;
        if generation > 0 {
            div()
                .size_full()
                .with_animation(
                    SharedString::from(format!("terminal-mode-switch-{generation}")),
                    Animation::new(Duration::from_millis(160)).with_easing(ease_out_quint()),
                    |element, delta| element.opacity(delta),
                )
                .child(root)
                .into_any_element()
        } else {
            root.into_any_element()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_are_recognised_from_command_lines() {
        let cases = [
            ("claude --resume", Some(TerminalAgent::Claude)),
            (
                "/Users/a/.local/share/claude/versions/2.1.5 --foo",
                Some(TerminalAgent::Claude),
            ),
            ("node /opt/homebrew/bin/gemini", Some(TerminalAgent::Gemini)),
            ("/usr/local/bin/codex", Some(TerminalAgent::Codex)),
            ("opencode", Some(TerminalAgent::OpenCode)),
            ("/usr/local/bin/agy", Some(TerminalAgent::Antigravity)),
            ("-zsh", None),
            ("/Applications/Claude.app/Contents/MacOS/Claude", None),
        ];
        for (command, expected) in cases {
            assert_eq!(TerminalAgent::from_command(command), expected, "{command}");
        }
    }

    #[test]
    fn layout_round_trips_through_json() {
        let layout = SavedTerminalLayout {
            mode: AppMode::Terminal,
            active: 1,
            font_size: Some(15.5),
            tabs: vec![
                SavedTerminalTab {
                    cwd: PathBuf::from("/tmp/a"),
                    history: "$ ls\nfile".into(),
                    resume: Some("herdr".into()),
                    child_pid: None,
                },
                SavedTerminalTab {
                    cwd: PathBuf::from("/tmp/b"),
                    history: String::new(),
                    resume: None,
                    child_pid: None,
                },
            ],
        };
        let json = serde_json::to_vec(&layout).unwrap();
        let back: SavedTerminalLayout = serde_json::from_slice(&json).unwrap();
        assert_eq!(back.mode, AppMode::Terminal);
        assert_eq!(back.active, 1);
        assert_eq!(back.font_size, Some(15.5));
        assert_eq!(back.tabs.len(), 2);
        assert_eq!(back.tabs[0].history, "$ ls\nfile");
        assert_eq!(back.tabs[0].resume.as_deref(), Some("herdr"));
        assert_eq!(back.tabs[1].resume, None);
    }

    #[test]
    fn only_allowlisted_programs_can_be_resumed() {
        assert_eq!(resumable_program("herdr"), Some("herdr"));
        assert_eq!(resumable_program("rm -rf ~"), None);
        assert_eq!(resumable_program("zsh"), None);
    }

    #[cfg(unix)]
    #[test]
    fn process_scan_finds_nothing_under_a_plain_process() {
        let found = resumable_programs_under(&[std::process::id()]);
        assert!(found.is_empty());
    }

    #[test]
    fn partial_or_empty_layout_defaults_to_agents_mode() {
        let back: SavedTerminalLayout = serde_json::from_str("{}").unwrap();
        assert_eq!(back.mode, AppMode::Agents);
        assert!(back.tabs.is_empty());
    }
}
