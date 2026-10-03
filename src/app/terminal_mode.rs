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

/// For each shell pid, the resumable program running anywhere under it.
/// One `ps` call covers every tab. Blocking, so callers run it off the UI
/// thread.
#[cfg(unix)]
fn resumable_programs_under(roots: &[u32]) -> HashMap<u32, &'static str> {
    let Ok(output) = std::process::Command::new("ps")
        .args(["-A", "-o", "pid=,ppid=,comm="])
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
                let name = Path::new(command)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_default();
                if let Some(program) = resumable_program(name) {
                    found.insert(*root, program);
                }
                pending.push(*child);
            }
        }
    }
    found
}

#[cfg(not(unix))]
fn resumable_programs_under(_roots: &[u32]) -> HashMap<u32, &'static str> {
    HashMap::new()
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
        self.terminal_tabs.push(TerminalTab {
            id: Uuid::new_v4(),
            view,
        });
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
        let mut strip = div()
            .id("terminal-mode-tabs")
            .h_full()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(3.0))
            .overflow_x_scroll();
        for (index, tab) in self.terminal_tabs.iter().enumerate() {
            let selected = index == self.active_terminal_tab;
            let view = tab.view.read(cx);
            let title = view.title().trim();
            let label = if !title.is_empty() {
                title.to_owned()
            } else {
                let cwd = view.working_directory();
                cwd.file_name()
                    .and_then(|name| name.to_str())
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("{} {}", tr!("right_panel.terminal"), index + 1))
            };
            let exited = view.is_exited();
            let tab_group = SharedString::from(format!("terminal-tab-{}", tab.id));
            strip = strip.child(
                div()
                    .id(tab_group.clone())
                    .group(tab_group.clone())
                    .tab_index(0)
                    .focus_visible(|element| element.border_color(theme.accent))
                    .h(px(28.0))
                    .min_w(px(54.0))
                    .max_w(px(160.0))
                    .pl(px(8.0))
                    .pr(px(6.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .text_size(sp(12.0))
                    .when(selected, |element| {
                        element
                            .bg(theme.raised)
                            .text_color(theme.text)
                            .font_weight(FontWeight::MEDIUM)
                            // A shadow shows through a translucent fill and
                            // makes the tab read as solid on glass styles.
                            .when(solid, |element| element.shadow_xs())
                    })
                    .when(!selected, |element| {
                        element
                            .bg(gpui::transparent_black())
                            .text_color(theme.text_tertiary)
                            .hover(|element| {
                                element.bg(theme.overlay).text_color(theme.text_secondary)
                            })
                    })
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
                    .child(
                        div()
                            .size(px(5.0))
                            .rounded_full()
                            .flex_none()
                            .bg(if exited {
                                theme.danger
                            } else if selected {
                                theme.accent
                            } else {
                                theme.text_ghost
                            }),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .truncate()
                            .child(SharedString::from(label)),
                    )
                    .child(
                        div()
                            .id(SharedString::from(format!("close-terminal-tab-{}", tab.id)))
                            .size(px(14.0))
                            .flex_none()
                            .rounded(px(3.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .opacity(0.0)
                            .group_hover(tab_group, |style| style.opacity(0.65))
                            .hover(|element| element.opacity(1.0).bg(theme.overlay_strong))
                            .child(icon("icons/x.svg", 8.0, theme.text_secondary))
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
                .size(px(24.0))
                .flex_none()
                .rounded(px(5.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|element| element.bg(theme.overlay))
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
                .child(icon("icons/plus.svg", 12.0, theme.text_tertiary))
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
        let body_width = f32::from(viewport.width);
        let body_height = (f32::from(viewport.height) - TERMINAL_TITLEBAR_HEIGHT).max(60.0);
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
                _ => theme.terminal,
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
                    .overflow_hidden()
                    .children(active),
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
