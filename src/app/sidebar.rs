use chrono::{DateTime, Datelike, Days, Local, NaiveDate, Utc};
use gpui::{KeyBinding, actions};
use std::time::Duration;

use super::right_panel::file_icon_for_path;
use super::*;

actions!(insulator_sidebar, [CancelSessionRename]);

const SESSION_RENAME_PARENT_CONTEXT: &str = "SessionRename";
const SESSION_RENAME_FIELD_CONTEXT: &str = "SessionRename > TextInput";
const SIDEBAR_SETTINGS_ICON_SIZE: f32 = 16.0;

/// Keep Escape inside the focused inline editor so it cancels the rename,
/// rather than falling through to the window-wide Stop action.
pub fn init(cx: &mut App) {
    cx.bind_keys([KeyBinding::new(
        "escape",
        CancelSessionRename,
        Some(SESSION_RENAME_FIELD_CONTEXT),
    )]);
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum SessionDateGroup {
    Today,
    Yesterday,
    ThisWeek,
    ThisMonth,
    ThisYear,
    More,
}

impl SessionDateGroup {
    const ALL: [Self; 6] = [
        Self::Today,
        Self::Yesterday,
        Self::ThisWeek,
        Self::ThisMonth,
        Self::ThisYear,
        Self::More,
    ];

    fn index(self) -> usize {
        match self {
            Self::Today => 0,
            Self::Yesterday => 1,
            Self::ThisWeek => 2,
            Self::ThisMonth => 3,
            Self::ThisYear => 4,
            Self::More => 5,
        }
    }

    fn label(self) -> String {
        match self {
            Self::Today => tr!("sidebar.today"),
            Self::Yesterday => tr!("sidebar.yesterday"),
            Self::ThisWeek => tr!("sidebar.this_week"),
            Self::ThisMonth => tr!("sidebar.this_month"),
            Self::ThisYear => tr!("sidebar.this_year"),
            Self::More => tr!("sidebar.more"),
        }
    }
}

/// Stable identity for a collapsible sidebar section. Keeping both variants in
/// one set preserves each view's disclosure state when the user switches
/// between Project and Updated grouping.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) enum SidebarGroup {
    Updated(SessionDateGroup),
    Project(Uuid),
    Projectless,
    Status(ChatStatus),
}

impl SidebarGroup {
    fn element_key(self) -> SharedString {
        match self {
            Self::Updated(group) => format!("updated-{}", group.index()).into(),
            Self::Project(project_id) => format!("project-{project_id}").into(),
            Self::Projectless => "projectless".into(),
            Self::Status(status) => format!("status-{}", status.as_str()).into(),
        }
    }

    fn mix_fingerprint(self, fingerprint: u64) -> u64 {
        match self {
            Self::Updated(group) => mix(fingerprint, group.index() as u64 + 1),
            Self::Project(project_id) => mix_uuid(mix(fingerprint, 0x100), project_id),
            Self::Projectless => mix(fingerprint, 0x200),
            Self::Status(status) => mix(fingerprint, 0x300 + status.index() as u64),
        }
    }
}

fn sidebar_grouping_label(grouping: SidebarGrouping) -> String {
    match grouping {
        SidebarGrouping::Project => "Project".to_owned(),
        SidebarGrouping::Chats => "Chats".to_owned(),
        SidebarGrouping::Status => "Status".to_owned(),
    }
}

fn sidebar_ordering_label(ordering: SidebarOrdering) -> String {
    match ordering {
        SidebarOrdering::Updated => "Updated".to_owned(),
        SidebarOrdering::Created => "Created".to_owned(),
    }
}

fn session_date_group(timestamp: u64, today: NaiveDate) -> SessionDateGroup {
    let session_date = i64::try_from(timestamp)
        .ok()
        .and_then(|timestamp| DateTime::<Utc>::from_timestamp(timestamp, 0))
        .map(|timestamp| timestamp.with_timezone(&Local).date_naive())
        .unwrap_or(today);
    session_date_group_for_dates(session_date, today)
}

fn session_date_group_for_dates(session_date: NaiveDate, today: NaiveDate) -> SessionDateGroup {
    if session_date >= today {
        return SessionDateGroup::Today;
    }

    if today.pred_opt() == Some(session_date) {
        return SessionDateGroup::Yesterday;
    }

    let week_start = today
        .checked_sub_days(Days::new(today.weekday().num_days_from_monday().into()))
        .unwrap_or(today);
    if session_date >= week_start {
        return SessionDateGroup::ThisWeek;
    }

    if session_date.year() == today.year() && session_date.month() == today.month() {
        return SessionDateGroup::ThisMonth;
    }

    if session_date.year() == today.year() {
        return SessionDateGroup::ThisYear;
    }

    SessionDateGroup::More
}

fn session_group_header(theme: &Theme) -> Div {
    div()
        .h(px(SIDEBAR_GROUP_HEADER_HEIGHT))
        .px(px(8.0))
        .flex()
        .items_center()
        .text_size(sp(13.0))
        .font_weight(FontWeight::MEDIUM)
        .text_color(theme.text_secondary)
}

fn sidebar_session_providers(session: &AgentSession) -> Vec<ProviderKind> {
    let mut providers = vec![session.provider];
    if let Some(provider) = session
        .provider_cursor
        .as_ref()
        .map(ProviderResumeCursor::provider)
        .filter(|provider| !providers.contains(provider))
    {
        providers.push(provider);
    }
    providers
}

fn sidebar_provider_mark(theme: &Theme, providers: &[ProviderKind]) -> Div {
    let visible = providers.len().min(3);
    let diameter = 18.0;
    let overlap = 7.0;
    let width = diameter + overlap * visible.saturating_sub(1) as f32;

    providers.iter().take(visible).enumerate().fold(
        div().relative().w(px(width)).h(px(diameter)).flex_none(),
        |stack, (index, provider)| {
            stack.child(
                div()
                    .absolute()
                    .left(px(overlap * index as f32))
                    .size(px(diameter))
                    .rounded_full()
                    .border_1()
                    .border_color(theme.sidebar)
                    .bg(theme.sidebar_item_background)
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(provider_mark(
                        theme,
                        *provider,
                        11.0,
                        provider_color(theme, *provider),
                    )),
            )
        },
    )
}

fn append_sidebar_group_rows(
    rows: &mut Vec<SidebarRow>,
    group: SidebarGroup,
    sessions: &[Uuid],
    collapsed: bool,
    show_more: bool,
) {
    rows.push(SidebarRow::Header(group));
    if !collapsed {
        rows.extend(sessions.iter().copied().map(SidebarRow::Session));
        if show_more {
            rows.push(SidebarRow::ShowMore(group));
        }
    }
    rows.push(SidebarRow::GroupSpacer);
}

fn updater_button_available_content(
    foreground: Hsla,
    label: SharedString,
    label_reveal: f32,
) -> Div {
    div()
        .relative()
        .size_full()
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .opacity(1.0 - label_reveal)
                .child(icon("icons/download.svg", 12.0, foreground)),
        )
        .child(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .whitespace_nowrap()
                .opacity(label_reveal)
                .child(label),
        )
}

/// Height of a session card plus the separation reserved beneath it in the
/// virtualized sidebar list. Keep the gap inside the list row so measured and
/// estimated heights stay identical for off-screen sessions.
const SIDEBAR_SESSION_CARD_HEIGHT: f32 = 32.0;
const SIDEBAR_SESSION_ROW_GAP: f32 = 2.0;
const SIDEBAR_SESSION_ROW_HEIGHT: f32 = SIDEBAR_SESSION_CARD_HEIGHT + SIDEBAR_SESSION_ROW_GAP;
const SIDEBAR_ACTION_ROW_HEIGHT: f32 = 34.0;
const SIDEBAR_ACTION_ICON_SIZE: f32 = 15.0;
const SIDEBAR_SEARCH_BOTTOM_GAP: f32 = 6.0;
const SIDEBAR_GROUP_HEADER_HEIGHT: f32 = 34.0;
const SIDEBAR_GROUP_HEADER_BOTTOM_GAP: f32 = 2.0;
const SIDEBAR_SHOW_MORE_ROW_HEIGHT: f32 = 28.0;
const SIDEBAR_GROUP_SPACER_HEIGHT: f32 = 4.0;
const SIDEBAR_GROUP_GUIDE_X: f32 = 15.0;
const SIDEBAR_GROUP_CHILD_PADDING: f32 = 28.0;
const SIDEBAR_PROJECT_RECENT_WINDOW_SECONDS: u64 = 3 * 24 * 60 * 60;
const SIDEBAR_PROJECT_REVEAL_BATCH: usize = 30;

/// The session row's trailing time: how long the live turn has been working,
/// or how long ago the agent last replied. A session that has never replied
/// shows nothing.
pub(super) fn session_time_label(session: &AgentSession, now: u64) -> Option<String> {
    if session.status == SessionStatus::Background {
        return Some(tr!("sidebar.status_background"));
    }
    if session.is_busy()
        && let Some(turn) = session
            .turns
            .last()
            .filter(|turn| turn.status == TurnStatus::Running)
    {
        return Some(tr!(
            "sidebar.working",
            elapsed = format_working_elapsed(now.saturating_sub(turn.started_at))
        ));
    }
    session
        .last_reply_at
        .map(|last_reply_at| format_time_ago(now.saturating_sub(last_reply_at)))
}

/// Recency for sidebar ordering and date groups. A submitted turn promotes the
/// task immediately, while metadata edits such as a rename do not; a task with
/// no turns stays anchored to when it was created.
fn sidebar_session_timestamp(session: &AgentSession) -> u64 {
    session.last_reply_at.unwrap_or(session.created_at)
}

fn sort_sidebar_sessions(sessions: &mut Vec<&AgentSession>, ordering: SidebarOrdering) {
    match ordering {
        SidebarOrdering::Updated => {
            sessions.sort_by_key(|session| std::cmp::Reverse(sidebar_session_timestamp(session)))
        }
        SidebarOrdering::Created => {
            sessions.sort_by_key(|session| std::cmp::Reverse(session.created_at))
        }
    }
}

fn project_sidebar_groups(
    sessions: &[&AgentSession],
    projectless_project_ids: &HashSet<Uuid>,
) -> Vec<(SidebarGroup, Vec<Uuid>)> {
    let mut groups: Vec<(SidebarGroup, Vec<Uuid>)> = Vec::new();
    let mut indexes = HashMap::new();
    let mut projectless_sessions = Vec::new();
    for session in sessions {
        if projectless_project_ids.contains(&session.project_id) {
            projectless_sessions.push(session.id);
            continue;
        }
        let index = *indexes.entry(session.project_id).or_insert_with(|| {
            let index = groups.len();
            groups.push((SidebarGroup::Project(session.project_id), Vec::new()));
            index
        });
        groups[index].1.push(session.id);
    }
    if !projectless_sessions.is_empty() {
        groups.push((SidebarGroup::Projectless, projectless_sessions));
    }
    groups
}

fn visible_project_sessions(
    sessions: &[Uuid],
    session_timestamps: &HashMap<Uuid, u64>,
    recent_cutoff: u64,
    revealed_older_sessions: usize,
) -> (Vec<Uuid>, bool) {
    let mut visible = Vec::with_capacity(sessions.len());
    let mut older_seen = 0usize;
    for session_id in sessions {
        let recent = session_timestamps
            .get(session_id)
            .is_some_and(|timestamp| *timestamp >= recent_cutoff);
        if recent || older_seen < revealed_older_sessions {
            visible.push(*session_id);
        }
        if !recent {
            older_seen = older_seen.saturating_add(1);
        }
    }
    (visible, older_seen > revealed_older_sessions)
}

fn sidebar_project_is_projectless(project: &Project, projectless_root: Option<&Path>) -> bool {
    projectless_root.is_some_and(|root| project.path.starts_with(root))
}

fn sidebar_project_should_show(_session_count: usize) -> bool {
    true
}

fn persisted_sidebar_branch_label(workspace: &SessionWorkspace) -> Option<&str> {
    match workspace {
        SessionWorkspace::Local => None,
        SessionWorkspace::NewWorktree { base_branch } => base_branch.as_deref(),
        SessionWorkspace::Worktree { branch, .. } => Some(branch.as_str()),
    }
    .filter(|branch| !branch.is_empty())
}

/// Compact "how long ago" for the sidebar: "just now", then one coarse unit —
/// "5m", "3h", "420d". Days are the largest unit so a glance still reads as a
/// count rather than a date.
pub(super) fn format_time_ago(seconds: u64) -> String {
    match seconds {
        0..=59 => tr!("sidebar.just_now"),
        60..=3_599 => tr!("sidebar.minutes_ago", count = seconds / 60),
        3_600..=86_399 => tr!("sidebar.hours_ago", count = seconds / 3_600),
        _ => tr!("sidebar.days_ago", count = seconds / 86_400),
    }
}

/// One row of the virtualized sidebar session history.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SidebarRow {
    /// Opens the window-wide command palette and scrolls with history.
    Search,
    /// Group header; the first row also carries the sidebar actions.
    Header(SidebarGroup),
    /// Heading separating pinned folders from ordinary workspaces.
    PinnedHeader,
    /// Heading for the complete workspace list and its actions.
    WorkspacesHeader,
    /// A started session in the main workspace tree.
    Session(Uuid),
    /// A started session duplicated in the pinned conversations section.
    PinnedSession(Uuid),
    /// Reveals the next batch of older sessions in a project section.
    ShowMore(SidebarGroup),
    /// Spacing between date groups.
    GroupSpacer,
}

fn sidebar_session_row_index(rows: &[SidebarRow], session_id: Uuid) -> Option<usize> {
    rows.iter()
        .position(|row| *row == SidebarRow::Session(session_id))
}

fn sidebar_row_height(row: SidebarRow) -> Pixels {
    px(match row {
        SidebarRow::Search => SIDEBAR_ACTION_ROW_HEIGHT + SIDEBAR_SEARCH_BOTTOM_GAP,
        SidebarRow::Header(_) => SIDEBAR_GROUP_HEADER_HEIGHT + SIDEBAR_GROUP_HEADER_BOTTOM_GAP,
        SidebarRow::PinnedHeader | SidebarRow::WorkspacesHeader => SIDEBAR_GROUP_HEADER_HEIGHT,
        SidebarRow::Session(_) | SidebarRow::PinnedSession(_) => SIDEBAR_SESSION_ROW_HEIGHT,
        SidebarRow::ShowMore(_) => SIDEBAR_SHOW_MORE_ROW_HEIGHT,
        SidebarRow::GroupSpacer => SIDEBAR_GROUP_SPACER_HEIGHT,
    })
}

fn sidebar_bottom_aligned_offset(
    rows: &[SidebarRow],
    target: usize,
    viewport_height: Pixels,
) -> ListOffset {
    let mut item_ix = target;
    let mut height = sidebar_row_height(rows[target]);
    while item_ix > 0 && height < viewport_height {
        item_ix -= 1;
        height += sidebar_row_height(rows[item_ix]);
    }
    ListOffset {
        item_ix,
        offset_in_item: (height - viewport_height).max(Pixels::ZERO),
    }
}

fn reveal_sidebar_list_row(list: &ListState, rows: &[SidebarRow], index: usize) {
    let viewport = list.viewport_bounds();
    if viewport.size.height <= Pixels::ZERO {
        return;
    }
    if let Some(item) = list.bounds_for_item(index) {
        if item.top() >= viewport.top() && item.bottom() <= viewport.bottom() {
            return;
        }
        list.scroll_to_reveal_item(index);
    } else if index <= list.logical_scroll_top().item_ix {
        list.scroll_to(ListOffset {
            item_ix: index,
            offset_in_item: Pixels::ZERO,
        });
    } else {
        // Off-screen rows have not necessarily been measured yet. Their
        // sidebar heights are fixed, so align a lower target to the viewport
        // bottom just like scrollIntoView({ block: "nearest" }).
        list.scroll_to(sidebar_bottom_aligned_offset(
            rows,
            index,
            viewport.size.height,
        ));
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DraggedSession {
    pub session_id: Uuid,
    pub status: ChatStatus,
    pub title: SharedString,
}

pub(super) struct DraggedSessionPreview {
    pub title: SharedString,
    pub status: ChatStatus,
}

impl Render for DraggedSessionPreview {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::current(cx);
        let status_color = chat_status_color(&theme, self.status);
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .px(px(10.0))
            .h(px(28.0))
            .max_w(px(240.0))
            .rounded(px(7.0))
            .bg(theme.raised)
            .border_1()
            .border_color(theme.border)
            .shadow_lg()
            .child(icon(self.status.icon(), 14.0, status_color))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(sp(13.0))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(theme.text)
                    .child(self.title.clone()),
            )
    }
}

impl Insulator {
    pub(super) fn window_drag_region(
        &self,
        region: Stateful<Div>,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        // Windows drags from the hit test, not from a mouse-move handler:
        // `DefWindowProc` moves the window once the region reports itself as
        // caption, and performs the user's configured double-click action.
        #[cfg(target_os = "windows")]
        let region = region.window_control_area(gpui::WindowControlArea::Drag);

        region
            .on_click(|event, window, _| {
                if event.click_count() == 2 {
                    crate::platform::titlebar_double_click(window);
                }
            })
            .on_mouse_down_out(cx.listener(|this, _, _, _| {
                this.header_drag_armed = false;
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.header_drag_armed = true;
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| {
                    this.header_drag_armed = false;
                }),
            )
            .on_mouse_move(cx.listener(|this, _, window, _| {
                if this.header_drag_armed {
                    this.header_drag_armed = false;
                    crate::platform::start_window_move(window);
                }
            }))
    }
    // ── Sidebar ────────────────────────────────────────────────────────────

    fn render_fps_counter(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::current(cx);
        let fps = self.fps_value;
        let dot = if fps == 0 {
            theme.text_ghost
        } else if fps >= 55 {
            theme.success
        } else if fps >= 30 {
            theme.warning
        } else {
            theme.danger
        };
        div()
            .flex_none()
            .h(px(26.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .text_size(sp(12.5))
            .line_height(sp(0.0))
            .child(div().w(px(6.0)).h(px(6.0)).rounded_full().bg(dot))
            .child(
                div()
                    .text_color(theme.text_tertiary)
                    .font_family(crate::md::render::active_mono_family())
                    .child(SharedString::from(format!("{fps} FPS"))),
            )
    }

    fn render_sidebar_toggle(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::current(cx);
        div()
            .id("toggle-sidebar")
            .w(px(26.0))
            .h(px(26.0))
            .flex_none()
            .rounded(px(6.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_default()
            .hover(|element| element.bg(theme.overlay))
            .active(|element| element.bg(theme.overlay_strong))
            .child(icon("icons/panel-left.svg", 14.0, theme.text_tertiary))
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.set_sidebar_visible(!this.sidebar_visible, cx);
            }))
    }

    pub(super) fn render_history_button(
        &self,
        id: &'static str,
        icon_path: &'static str,
        enabled: bool,
        navigate_back: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::current(cx);
        div()
            .id(id)
            .w(px(26.0))
            .h(px(26.0))
            .flex_none()
            .rounded(px(6.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_default()
            .when(!enabled, |element| element.opacity(0.35))
            .when(enabled, |element| {
                element
                    .hover(|element| element.bg(theme.overlay))
                    .active(|element| element.bg(theme.overlay_strong))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                        cx.stop_propagation();
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        if navigate_back {
                            this.navigate_back_action(&NavigateBack, window, cx);
                        } else {
                            this.navigate_forward_action(&NavigateForward, window, cx);
                        }
                    }))
            })
            .child(icon(icon_path, 14.0, theme.text_tertiary))
    }

    fn render_sidebar_titlebar(&self, window: &Window, cx: &mut Context<Self>) -> Stateful<Div> {
        div()
            .id("sidebar-titlebar")
            .h(px(48.0))
            .flex_none()
            .flex()
            .items_center()
            .children(self.render_client_window_controls(
                super::window_chrome::WindowControlSide::Left,
                window,
                cx,
            ))
            .child(
                self.window_drag_region(
                    div()
                        .id("sidebar-traffic-light-drag-region")
                        .w(px(TRAFFIC_LIGHT_CLEARANCE))
                        .h_full()
                        .flex_none(),
                    cx,
                ),
            )
            .child(self.render_sidebar_toggle(cx))
            .when(self.state.show_resource_usage, |bar| {
                bar.child(
                    div()
                        .ml(px(4.0))
                        .child(self.render_resource_usage_button(cx)),
                )
            })
            .child(
                div()
                    .ml(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(self.render_history_button(
                        "navigate-back",
                        "icons/arrow-left.svg",
                        !self.session_navigation.back.is_empty(),
                        true,
                        cx,
                    ))
                    .child(self.render_history_button(
                        "navigate-forward",
                        "icons/arrow-right.svg",
                        !self.session_navigation.forward.is_empty(),
                        false,
                        cx,
                    )),
            )
            .child(self.window_drag_region(
                div().id("sidebar-titlebar-drag-region").h_full().flex_1(),
                cx,
            ))
    }

    fn render_sidebar_header_actions(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::current(cx);
        let options_menu = self.menu_handle("sidebar-options", cx);
        let options_weak = cx.entity().downgrade();
        let grouping = self.state.sidebar_grouping;
        let ordering = self.state.sidebar_ordering;
        let options = dropdown_menu(
            div()
                .id("sidebar-options")
                .tab_index(0)
                .w(px(22.0))
                .h(px(22.0))
                .rounded(px(5.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_default()
                .focus_visible(|style| style.border_1().border_color(theme.accent))
                .when(options_menu.is_open(), |element| {
                    element.bg(theme.overlay_strong)
                })
                .hover(|element| element.bg(theme.overlay))
                .active(|element| element.bg(theme.overlay_strong))
                .tooltip(Tooltip::text("Sort and group"))
                .child(icon("icons/list-filter.svg", 14.0, theme.text_secondary))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            "sidebar-options-menu",
            &options_menu,
            MenuAlign::BelowLeft,
            move |_| {
                let group_weak = options_weak.clone();
                let sort_weak = options_weak.clone();

                vec![
                    MenuItem::submenu_with_value(
                        "Group by",
                        sidebar_grouping_label(grouping),
                        move |_| {
                            let project_weak = group_weak.clone();
                            let chats_weak = group_weak.clone();
                            let status_weak = group_weak.clone();
                            vec![
                                MenuItem::new("Project", move |_, cx| {
                                    let _ = project_weak.update(cx, |this, cx| {
                                        this.set_sidebar_grouping(SidebarGrouping::Project, cx);
                                    });
                                })
                                .selected(grouping == SidebarGrouping::Project),
                                MenuItem::new("Chats", move |_, cx| {
                                    let _ = chats_weak.update(cx, |this, cx| {
                                        this.set_sidebar_grouping(SidebarGrouping::Chats, cx);
                                    });
                                })
                                .selected(grouping == SidebarGrouping::Chats),
                                MenuItem::new("Status", move |_, cx| {
                                    let _ = status_weak.update(cx, |this, cx| {
                                        this.set_sidebar_grouping(SidebarGrouping::Status, cx);
                                    });
                                })
                                .selected(grouping == SidebarGrouping::Status),
                            ]
                        },
                    ),
                    MenuItem::submenu_with_value(
                        "Sort by",
                        sidebar_ordering_label(ordering),
                        move |_| {
                            let updated_weak = sort_weak.clone();
                            let created_weak = sort_weak.clone();
                            vec![
                                MenuItem::new("Updated", move |_, cx| {
                                    let _ = updated_weak.update(cx, |this, cx| {
                                        this.set_sidebar_ordering(SidebarOrdering::Updated, cx);
                                    });
                                })
                                .selected(ordering == SidebarOrdering::Updated),
                                MenuItem::new("Created", move |_, cx| {
                                    let _ = created_weak.update(cx, |this, cx| {
                                        this.set_sidebar_ordering(SidebarOrdering::Created, cx);
                                    });
                                })
                                .selected(ordering == SidebarOrdering::Created),
                            ]
                        },
                    ),
                ]
            },
        );

        let add_project_menu = self.menu_handle("add-project", cx);
        let add_project_weak = cx.entity().downgrade();
        let local_project = add_project_weak.clone();
        let github_project = add_project_weak.clone();
        let add_project = dropdown_menu(
            div()
                .id("add-project")
                .tab_index(0)
                .w(px(20.0))
                .h(px(22.0))
                .rounded(px(6.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_default()
                .focus_visible(|style| style.border_1().border_color(theme.accent))
                .hover(|element| element.bg(theme.overlay))
                .active(|element| element.bg(theme.overlay_strong))
                .tooltip(Tooltip::text(tr!("project.new_project")))
                .child(icon("icons/folder-new.svg", 14.0, theme.text_secondary))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()),
            "add-project-menu",
            &add_project_menu,
            MenuAlign::BelowRight,
            move |_| {
                let local_project = local_project.clone();
                let github_project = github_project.clone();
                vec![
                    MenuItem::new("Local project", move |_, cx| {
                        let _ = local_project.update(cx, |insulator, cx| insulator.add_project(cx));
                    })
                    .icon("icons/folder-new.svg"),
                    MenuItem::new("Import from GitHub", move |window, cx| {
                        let _ = github_project.update(cx, |insulator, cx| {
                            insulator.open_github_project_dialog(window, cx)
                        });
                    })
                    .icon("icons/github.svg"),
                ]
            },
        );

        div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(options)
            .child(add_project)
    }

    fn render_sidebar_action_row(
        &self,
        id: &'static str,
        icon_path: &'static str,
        label: String,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let theme = Theme::current(cx);
        div()
            .id(id)
            .tab_index(0)
            .w_full()
            .h(px(SIDEBAR_ACTION_ROW_HEIGHT))
            .flex_none()
            .px(px(8.0))
            .rounded(px(7.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .cursor_default()
            .focus_visible(|style| style.border_1().border_color(theme.accent))
            .hover(|element| element.bg(theme.sidebar_item_background))
            .active(|element| element.bg(theme.overlay_strong))
            .child(
                div()
                    .size(px(20.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon(
                        icon_path,
                        SIDEBAR_ACTION_ICON_SIZE,
                        theme.text_secondary,
                    )),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(sp(13.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(label),
            )
    }

    fn render_sidebar_pull_requests(&self, cx: &mut Context<Self>) -> Div {
        let pull_requests = self
            .render_sidebar_action_row(
                "sidebar-pull-requests",
                "icons/git-branch.svg",
                "Pull requests".to_owned(),
                cx,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.open_pull_requests(cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.open_pull_requests(cx);
                    cx.stop_propagation();
                }
            }));
        div()
            .w_full()
            .h(px(SIDEBAR_ACTION_ROW_HEIGHT))
            .flex_none()
            .child(pull_requests)
    }

    pub(super) fn open_pull_requests(&mut self, cx: &mut Context<Self>) {
        self.pull_requests_open = true;
        self.state.selected_session = None;
        self.active_main_file_tab = None;
        self.active_main_review_tab = false;
        self.ensure_pull_requests(false, cx);
        self.save();
        cx.notify();
    }

    fn render_sidebar_new_task(&self, cx: &mut Context<Self>) -> Div {
        let new_task = self
            .render_sidebar_action_row(
                "sidebar-new-task",
                "icons/new-task.svg",
                tr!("menu.new_task"),
                cx,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.new_session_action(&NewSession, window, cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.new_session_action(&NewSession, window, cx);
                    cx.stop_propagation();
                }
            }));
        div()
            .w_full()
            .h(px(SIDEBAR_ACTION_ROW_HEIGHT))
            .flex_none()
            .child(new_task)
    }

    fn render_sidebar_home(&self, cx: &mut Context<Self>) -> Div {
        let home = self
            .render_sidebar_action_row("sidebar-home", "icons/home.svg", tr!("sidebar.home"), cx)
            .on_click(cx.listener(|this, _, _, cx| {
                this.open_home_screen(cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.open_home_screen(cx);
                    cx.stop_propagation();
                }
            }));
        div()
            .w_full()
            .h(px(SIDEBAR_ACTION_ROW_HEIGHT))
            .flex_none()
            .child(home)
    }

    pub(super) fn open_home_screen(&mut self, cx: &mut Context<Self>) {
        self.state.selected_session = None;
        self.state.selected_project = None;
        self.pull_requests_open = false;
        self.active_main_file_tab = None;
        self.active_main_review_tab = false;
        self.main_tabs.clear();
        self.main_tabs_open = false;
        self.save();
        cx.notify();
    }

    fn render_sidebar_search(&self, cx: &mut Context<Self>) -> Div {
        let search = self
            .render_sidebar_action_row(
                "sidebar-search",
                "icons/search.svg",
                tr!("sidebar.search"),
                cx,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.toggle_command_palette_action(&ToggleCommandPalette, window, cx);
            }))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, window, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.toggle_command_palette_action(&ToggleCommandPalette, window, cx);
                    cx.stop_propagation();
                }
            }));
        div()
            .w_full()
            .h(px(SIDEBAR_ACTION_ROW_HEIGHT + SIDEBAR_SEARCH_BOTTOM_GAP))
            .flex_none()
            .child(search)
    }

    fn start_available_update(&mut self, cx: &mut Context<Self>) {
        if self.updater_status != crate::updater::UpdateStatus::Available {
            return;
        }
        let started = cx
            .try_global::<crate::updater::UpdaterState>()
            .and_then(|state| state.0.as_ref())
            .is_some_and(|updater| updater.install_available_update());
        if started {
            self.updater_status = crate::updater::UpdateStatus::Updating;
            self.reset_updater_button_animation();
            cx.notify();
        }
    }

    fn render_updater_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let status = self.updater_status;
        if status == crate::updater::UpdateStatus::Idle {
            return None;
        }

        let theme = Theme::current(cx);
        let foreground = rgb(0xFFFFFF).into();
        let available = status == crate::updater::UpdateStatus::Available;
        let button = div()
            .id("sidebar-update")
            .track_focus(&self.updater_button_focus)
            .when(available, |button| button.tab_index(0))
            .w(px(UPDATER_BUTTON_COLLAPSED_WIDTH))
            .h(px(20.0))
            .flex_none()
            .overflow_hidden()
            .rounded_full()
            .relative()
            .cursor_default()
            .bg(theme.gauge)
            .text_color(foreground)
            .text_size(sp(12.5))
            .font_weight(FontWeight::MEDIUM)
            .when(available, |button| {
                button
                    .hover(|style| style.opacity(0.92))
                    .focus_visible(|style| style.border_1().border_color(rgb(0xFFFFFF)))
                    .active(|style| style.opacity(0.8))
                    .on_hover(cx.listener(|this, hovering: &bool, _, cx| {
                        this.set_updater_button_hovered(*hovering, cx);
                    }))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.start_available_update(cx);
                    }))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.start_available_update(cx);
                            cx.stop_propagation();
                        }
                    }))
            });

        if !available {
            let indicator = dot_matrix_loader(foreground, 14.0);
            return Some(
                button
                    .tooltip(Tooltip::text(tr!("updater.updating")))
                    .child(
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(indicator),
                    )
                    .into_any_element(),
            );
        }

        let label: SharedString = tr_cow!("updater.update").into();
        let animation_generation = self.updater_button_animation_generation;
        if animation_generation == 0 {
            return Some(
                button
                    .child(updater_button_available_content(foreground, label, 0.0))
                    .into_any_element(),
            );
        }

        let from_width = self.updater_button_animation_from_width;
        let from_reveal = self.updater_button_animation_from_reveal;
        let target_width = if self.updater_button_expanded() {
            UPDATER_BUTTON_EXPANDED_WIDTH
        } else {
            UPDATER_BUTTON_COLLAPSED_WIDTH
        };
        let target_reveal = if self.updater_button_expanded() {
            1.0
        } else {
            0.0
        };
        let current_width = self.updater_button_width.clone();
        let current_reveal = self.updater_button_label_reveal.clone();

        Some(
            button
                .with_animation(
                    SharedString::from(format!("sidebar-updater-expand-{animation_generation}")),
                    Animation::new(Duration::from_millis(150)).with_easing(ease_out_quint()),
                    move |button, delta| {
                        let width = from_width + (target_width - from_width) * delta;
                        let reveal = from_reveal + (target_reveal - from_reveal) * delta;
                        current_width.set(width);
                        current_reveal.set(reveal);
                        button.w(px(width)).child(updater_button_available_content(
                            foreground,
                            label.clone(),
                            reveal,
                        ))
                    },
                )
                .into_any_element(),
        )
    }

    fn render_sidebar_footer(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::current(cx);
        div()
            .flex_none()
            .h(px(40.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .child(
                div()
                    .id("open-settings")
                    .tab_index(0)
                    .focus_visible(|style| style.border_1().border_color(theme.accent))
                    .w(px(26.0))
                    .h(px(26.0))
                    .flex_none()
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_default()
                    .hover(|element| element.bg(theme.overlay))
                    .active(|element| element.bg(theme.overlay_strong))
                    .tooltip(Tooltip::text(tr_cow!("common.settings")))
                    .child(icon(
                        "icons/settings.svg",
                        SIDEBAR_SETTINGS_ICON_SIZE,
                        theme.text_tertiary,
                    ))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_settings_action(&OpenSettings, window, cx);
                    })),
            )
            .child(div().flex_1())
            .when_some(self.render_updater_button(cx), |footer, button| {
                footer.child(button)
            })
    }

    /// Resolve every ordinary local project's branch in one background pass.
    /// The render path only computes an allocation-free source fingerprint;
    /// collection building and daemon requests happen once when that moves.
    fn ensure_sidebar_branch_labels(&self, cx: &mut Context<Self>) {
        if self.state.sidebar_grouping != SidebarGrouping::Project {
            return;
        }

        let mut fingerprint = 0xb4a7_c4e5_51de_ba11;
        for session in &self.state.sessions {
            if session.has_started() && matches!(&session.workspace, SessionWorkspace::Local) {
                fingerprint = mix_uuid(fingerprint, session.id);
                fingerprint = mix_uuid(fingerprint, session.project_id);
            }
        }
        for project in &self.state.projects {
            fingerprint = mix_uuid(fingerprint, project.id);
        }
        if self.sidebar_branch_scan_fingerprint.get() == Some(fingerprint) {
            return;
        }
        self.sidebar_branch_scan_fingerprint.set(Some(fingerprint));
        let generation = self.sidebar_branch_scan_generation.get().wrapping_add(1);
        self.sidebar_branch_scan_generation.set(generation);

        let local_project_ids = self
            .state
            .sessions
            .iter()
            .filter(|session| {
                session.has_started() && matches!(&session.workspace, SessionWorkspace::Local)
            })
            .map(|session| session.project_id)
            .collect::<HashSet<_>>();
        let projectless_root = crate::projectless::workspace_root();
        let paths = self
            .state
            .projects
            .iter()
            .filter(|project| local_project_ids.contains(&project.id))
            .filter(|project| !sidebar_project_is_projectless(project, projectless_root.as_deref()))
            .map(|project| project.path.clone())
            .collect::<HashSet<_>>();
        if paths.is_empty() {
            self.sidebar_branch_labels.borrow_mut().clear();
            return;
        }

        let workspace = insulator_client::WorkspaceClient::new(self.daemon.client());
        cx.spawn(async move |insulator, cx| {
            let labels = cx
                .background_executor()
                .spawn(async move {
                    let mut labels = HashMap::new();
                    for path in paths {
                        let branch = match workspace.request(
                            insulator_client::WorkspaceOperation::InspectBranches {
                                cwd: path.clone(),
                            },
                        ) {
                            Ok(insulator_client::WorkspaceResult::Branches {
                                snapshot: Some(snapshot),
                            }) => snapshot.display_branch().map(str::to_owned),
                            _ => None,
                        };
                        if let Some(branch) = branch {
                            labels.insert(path, branch);
                        }
                    }
                    labels
                })
                .await;
            let _ = insulator.update(cx, |insulator, cx| {
                if insulator.sidebar_branch_scan_generation.get() != generation {
                    return;
                }
                *insulator.sidebar_branch_labels.borrow_mut() = labels
                    .into_iter()
                    .map(|(path, branch)| (path, SharedString::from(branch)))
                    .collect();
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn cache_sidebar_branch_label(&self, path: &Path, branch: Option<&str>) {
        let mut labels = self.sidebar_branch_labels.borrow_mut();
        if let Some(branch) = branch.filter(|branch| !branch.is_empty()) {
            labels.insert(path.to_path_buf(), SharedString::from(branch.to_owned()));
        } else {
            labels.remove(path);
        }
    }

    pub(super) fn render_sidebar(
        &self,
        width: f32,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::current(cx);
        self.ensure_sidebar_branch_labels(cx);
        let is_resizing = self
            .panel_resize_drag
            .is_some_and(|drag| drag.target == PanelResizeTarget::Sidebar);
        let sidebar_background = theme.sidebar_background(
            self.state.window_style,
            self.state.sidebar_transparency,
            is_resizing,
        );
        let logo = if theme.is_dark {
            crate::assets::home_logo()
        } else {
            crate::assets::light_home_logo()
        };

        let rows = self.sidebar_rows_cached(Local::now().date_naive(), unix_time());
        self.sync_sidebar_rows(&rows);
        // Restored selection exists before ListState knows the viewport size.
        // Retry after the first layout so nearest-edge alignment has a height.
        if self.sidebar_list_state.viewport_bounds().size.height <= Pixels::ZERO
            && let Some(session_id) = self
                .pending_session_activation
                .map(|pending| pending.session_id)
                .or(self.state.selected_session)
        {
            let entity = cx.entity().downgrade();
            window.on_next_frame(move |_, cx| {
                let _ = entity.update(cx, |this, cx| {
                    let selected_session = this
                        .pending_session_activation
                        .map(|pending| pending.session_id)
                        .or(this.state.selected_session);
                    if selected_session == Some(session_id) {
                        this.reveal_sidebar_session(session_id);
                        cx.notify();
                    }
                });
            });
        }
        let history_scrolled =
            self.sidebar_list_state.scroll_px_offset_for_scrollbar().y < px(-0.5);
        let entity = cx.entity().downgrade();

        div()
            .w(px(width))
            .h_full()
            .flex_none()
            .flex()
            .flex_col()
            .bg(sidebar_background)
            .child(self.render_sidebar_titlebar(window, cx))
            .child(
                div()
                    .id("sidebar-brand")
                    .w_full()
                    .h(px(38.0))
                    .flex()
                    .items_center()
                    .justify_start()
                    .pl(px(22.0))
                    .child(
                        img(logo)
                            .w(px(120.0))
                            .h(px(41.0))
                            .object_fit(ObjectFit::Contain),
                    ),
            )
            .child(
                div()
                    .w_full()
                    .px(px(10.0))
                    .mt(px(6.0))
                    .child(self.render_sidebar_home(cx))
                    .child(self.render_sidebar_new_task(cx))
                    .child(self.render_sidebar_pull_requests(cx))
                    .child(self.render_sidebar_search(cx)),
            )
            .child(
                div()
                    .mx(px(10.0))
                    .mt(px(8.0))
                    .mb(px(6.0))
                    .h(px(1.0))
                    .bg(theme.border),
            )
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .child(
                        div().px(px(10.0)).size_full().child(
                            list(
                                self.sidebar_list_state.clone(),
                                move |index, _window, cx| {
                                    entity
                                        .upgrade()
                                        .map(|entity| {
                                            entity.update(cx, |this, cx| {
                                                this.sidebar_row(index, &rows, cx)
                                            })
                                        })
                                        .unwrap_or_else(|| div().into_any_element())
                                },
                            )
                            .size_full(),
                        ),
                    )
                    .child(scrollbar::vertical(
                        &self.sidebar_list_state,
                        &self.sidebar_scrollbar,
                    ))
                    .when(history_scrolled, |scroll| {
                        scroll.child(
                            div()
                                .absolute()
                                .top_0()
                                .left_0()
                                .w_full()
                                .h(px(1.0))
                                .bg(theme.border),
                        )
                    }),
            )
            .child(self.render_sidebar_footer(cx))
    }

    /// Keep a newly selected task visible without disturbing the sidebar when
    /// its row is already fully inside the viewport.
    pub(super) fn reveal_sidebar_session(&self, session_id: Uuid) {
        let rows = self.sidebar_rows_cached(Local::now().date_naive(), unix_time());
        self.sync_sidebar_rows(&rows);
        if let Some(index) = sidebar_session_row_index(&rows, session_id) {
            reveal_sidebar_list_row(&self.sidebar_list_state, &rows, index);
        }
    }

    /// The sidebar row snapshot, rebuilt only when its inputs move.
    ///
    /// The sidebar re-renders at pulse cadence whenever one of its session
    /// rows shows a working spinner, and rebuilding the snapshot sorts every
    /// started session and runs calendar math per session — far too much per
    /// tick for values that move at most once per stream commit. The
    /// fingerprint is an allocation-free scan of exactly what
    /// [`Self::sidebar_rows`] reads: started sessions with their project and
    /// recency, the presentation preferences, the collapsed-group set, and
    /// today's date and the moving project-recency boundary.
    fn sidebar_rows_cached(&self, today: NaiveDate, now: u64) -> Rc<Vec<SidebarRow>> {
        let mut fingerprint = mix(0x51de_ba5e_5eed_c0de, today.num_days_from_ce() as u64);
        fingerprint = mix(
            fingerprint,
            match self.state.sidebar_grouping {
                SidebarGrouping::Project => 1,
                SidebarGrouping::Chats => 2,
                SidebarGrouping::Status => 3,
            },
        );
        fingerprint = mix(
            fingerprint,
            match self.state.sidebar_ordering {
                SidebarOrdering::Updated => 1,
                SidebarOrdering::Created => 2,
            },
        );
        for session in &self.state.sessions {
            fingerprint = mix_uuid(fingerprint, session.id);
            fingerprint = mix_uuid(fingerprint, session.project_id);
            fingerprint = mix(fingerprint, sidebar_session_timestamp(session));
            fingerprint = mix(fingerprint, session.chat_status.index() as u64);
        }
        for session_id in &self.pinned_session_ids {
            fingerprint = mix_uuid(fingerprint, *session_id);
        }
        let expanded = self
            .sidebar_expanded_groups
            .iter()
            .fold(0u64, |combined, group| {
                combined.wrapping_add(group.mix_fingerprint(0))
            });
        fingerprint = mix(
            mix(fingerprint, self.sidebar_expanded_groups.len() as u64),
            expanded,
        );
        if let Some(selected) = self.state.selected_session {
            fingerprint = mix_uuid(fingerprint, selected);
        }
        if self.sidebar_rows_fingerprint.get() != Some(fingerprint) {
            *self.sidebar_rows_snapshot.borrow_mut() = Rc::new(self.sidebar_rows(today, now));
            self.sidebar_rows_fingerprint.set(Some(fingerprint));
        }
        self.sidebar_rows_snapshot.borrow().clone()
    }

    /// Snapshot the session history as a flat list of lightweight rows under
    /// the current grouping and ordering preferences.
    fn sidebar_rows(&self, _today: NaiveDate, _now: u64) -> Vec<SidebarRow> {
        let mut rows = Vec::new();
        let projectless_root = crate::projectless::workspace_root();

        let mut sessions = self
            .state
            .sessions
            .iter()
            .filter(|session| {
                if session.conversation_root_id.is_some() {
                    return false;
                }
                let is_projectless = self.state.projects.iter().any(|project| {
                    project.id == session.project_id
                        && sidebar_project_is_projectless(project, projectless_root.as_deref())
                });
                if is_projectless {
                    session.has_started() || self.state.selected_session == Some(session.id)
                } else {
                    session.has_started()
                }
            })
            .collect::<Vec<_>>();
        sort_sidebar_sessions(&mut sessions, self.state.sidebar_ordering);

        let pinned_sessions = sessions
            .iter()
            .filter(|session| self.pinned_session_ids.contains(&session.id))
            .map(|session| session.id)
            .collect::<Vec<_>>();
        if !pinned_sessions.is_empty() {
            rows.push(SidebarRow::PinnedHeader);
            rows.extend(pinned_sessions.into_iter().map(SidebarRow::PinnedSession));
            rows.push(SidebarRow::GroupSpacer);
        }
        rows.push(SidebarRow::WorkspacesHeader);

        match self.state.sidebar_grouping {
            SidebarGrouping::Chats => {
                for session in sessions {
                    rows.push(SidebarRow::Session(session.id));
                }
            }
            SidebarGrouping::Status => {
                for &status in &ChatStatus::GROUP_ORDER {
                    let group = SidebarGroup::Status(status);
                    let is_collapsed = !self.sidebar_expanded_groups.contains(&group);
                    let session_ids = sessions
                        .iter()
                        .filter(|session| session.chat_status == status)
                        .map(|session| session.id)
                        .collect::<Vec<_>>();
                    append_sidebar_group_rows(&mut rows, group, &session_ids, is_collapsed, false);
                }
            }
            SidebarGrouping::Project => {
                let mut sessions_by_project: HashMap<Uuid, Vec<&AgentSession>> = HashMap::new();
                let mut projectless_sessions: Vec<&AgentSession> = Vec::new();

                for session in sessions {
                    let is_projectless = self.state.projects.iter().any(|project| {
                        project.id == session.project_id
                            && sidebar_project_is_projectless(project, projectless_root.as_deref())
                    });
                    if is_projectless {
                        projectless_sessions.push(session);
                    } else {
                        sessions_by_project
                            .entry(session.project_id)
                            .or_default()
                            .push(session);
                    }
                }

                for project in &self.state.projects {
                    if sidebar_project_is_projectless(project, projectless_root.as_deref()) {
                        continue;
                    }
                    let group = SidebarGroup::Project(project.id);
                    let is_collapsed = !self.sidebar_expanded_groups.contains(&group);
                    let mut project_sessions =
                        sessions_by_project.remove(&project.id).unwrap_or_default();
                    if !sidebar_project_should_show(project_sessions.len()) {
                        continue;
                    }
                    sort_sidebar_sessions(&mut project_sessions, self.state.sidebar_ordering);
                    let session_ids = project_sessions.iter().map(|s| s.id).collect::<Vec<_>>();

                    append_sidebar_group_rows(&mut rows, group, &session_ids, is_collapsed, false);
                }

                if !projectless_sessions.is_empty() {
                    let group = SidebarGroup::Projectless;
                    let is_collapsed = !self.sidebar_expanded_groups.contains(&group);
                    let session_ids = projectless_sessions
                        .iter()
                        .map(|s| s.id)
                        .collect::<Vec<_>>();
                    append_sidebar_group_rows(&mut rows, group, &session_ids, is_collapsed, false);
                }
            }
        }

        rows
    }

    /// Keep the virtualized list in sync with the current row snapshot.
    /// Rows are cheap values, so only the minimal changed suffix is spliced,
    /// preserving scroll position and measured heights across unrelated churn
    /// (e.g. the active session's `updated_at` bumping on every stream tick).
    fn sync_sidebar_rows(&self, rows: &[SidebarRow]) {
        let mut cached = self.sidebar_row_cache.borrow_mut();
        if cached.as_slice() == rows {
            return;
        }
        let prefix = cached
            .iter()
            .zip(rows.iter())
            .take_while(|(a, b)| a == b)
            .count();
        let old_count = cached.len();
        *cached = rows.to_vec();
        if old_count == 0 {
            self.sidebar_list_state
                .reset_with_uniform_height(rows.len(), px(SIDEBAR_SESSION_ROW_HEIGHT));
        } else {
            self.sidebar_list_state
                .splice(prefix..old_count, rows.len() - prefix);
            // Newly inserted rows have no measured height yet; give them the
            // uniform hint so the scrollbar keeps a correct total height.
            self.sidebar_list_state
                .clone()
                .with_uniform_item_height(px(SIDEBAR_SESSION_ROW_HEIGHT));
        }
    }

    fn sidebar_row(&self, index: usize, rows: &[SidebarRow], cx: &mut Context<Self>) -> AnyElement {
        let Some(row) = rows.get(index) else {
            return div().into_any_element();
        };
        match *row {
            SidebarRow::Search => self.render_sidebar_search(cx).into_any_element(),
            SidebarRow::PinnedHeader => self
                .render_sidebar_section_header("Pinned", None, cx)
                .into_any_element(),
            SidebarRow::WorkspacesHeader => self
                .render_sidebar_section_header(
                    "My Workspaces",
                    Some(self.render_sidebar_header_actions(cx)),
                    cx,
                )
                .into_any_element(),
            SidebarRow::Header(group) => {
                let has_expanded_children = rows.get(index + 1).is_some_and(|row| {
                    matches!(
                        row,
                        SidebarRow::Session(_)
                            | SidebarRow::PinnedSession(_)
                            | SidebarRow::ShowMore(_)
                    )
                });
                self.render_sidebar_group_header(
                    group,
                    index
                        == if self.state.sidebar_grouping == SidebarGrouping::Project {
                            0
                        } else {
                            1
                        },
                    has_expanded_children,
                    cx,
                )
                .into_any_element()
            }
            SidebarRow::Session(session_id) => self
                .render_sidebar_session_item(session_id, false, cx)
                .into_any_element(),
            SidebarRow::PinnedSession(session_id) => self
                .render_sidebar_session_item(session_id, true, cx)
                .into_any_element(),
            SidebarRow::ShowMore(group) => {
                self.render_sidebar_show_more(group, cx).into_any_element()
            }
            SidebarRow::GroupSpacer => div()
                .w_full()
                .h(px(SIDEBAR_GROUP_SPACER_HEIGHT))
                .into_any_element(),
        }
    }

    fn render_sidebar_section_header(
        &self,
        label: &'static str,
        actions: Option<Div>,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::current(cx);
        div()
            .w_full()
            .h(px(SIDEBAR_GROUP_HEADER_HEIGHT))
            .px(px(8.0))
            .flex()
            .items_center()
            .justify_between()
            .text_size(sp(13.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text_secondary)
            .child(label)
            .children(actions)
    }

    fn render_sidebar_group_header(
        &self,
        group: SidebarGroup,
        _first: bool,
        _has_expanded_children: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::current(cx);
        let collapsed = !self.sidebar_expanded_groups.contains(&group);
        let group_key = group.element_key();
        let group_name = SharedString::from(format!("sidebar-group-header-{group_key}"));
        let header_focus = self
            .sidebar_group_header_focuses
            .borrow_mut()
            .entry(group)
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let show_folder_icon =
            matches!(group, SidebarGroup::Project(_) | SidebarGroup::Projectless);
        let label = match group {
            SidebarGroup::Updated(group) => group.label(),
            SidebarGroup::Project(project_id) => self
                .state
                .projects
                .iter()
                .find(|project| project.id == project_id)
                .map(Project::display_name)
                .unwrap_or_else(|| tr!("project.no_project_name")),
            SidebarGroup::Projectless => tr!("project.no_project_name"),
            SidebarGroup::Status(status) => status.label().into(),
        };
        let compose = show_folder_icon.then(|| {
            let compose_focus = self
                .sidebar_group_compose_focuses
                .borrow_mut()
                .entry(group)
                .or_insert_with(|| cx.focus_handle())
                .clone();
            div()
                .w(px(22.0))
                .h(px(22.0))
                .flex_none()
                .flex()
                .items_center()
                .justify_end()
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "sidebar-group-compose-{group_key}"
                        )))
                        .track_focus(&compose_focus)
                        .tab_index(0)
                        .tab_stop(true)
                        .w(px(20.0))
                        .h(px(20.0))
                        .rounded(px(4.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_default()
                        .focus_visible(|style| style.border_1().border_color(theme.accent))
                        .hover(|style| style.bg(theme.overlay))
                        .active(|style| style.bg(theme.overlay_strong))
                        .tooltip(Tooltip::text("New chat"))
                        .child(icon("icons/plus.svg", 13.0, theme.text_secondary))
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.open_new_task_for_sidebar_group(group, window, cx);
                        }))
                        .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                            if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                this.open_new_task_for_sidebar_group(group, window, cx);
                                cx.stop_propagation();
                            }
                        })),
                )
        });

        let avatar_path = crate::platform::local_user_github_avatar_path();
        let project_icon_el = if let Some(path) = avatar_path.as_ref() {
            img(path.clone())
                .size(px(22.0))
                .rounded(px(5.0))
                .flex_none()
                .into_any_element()
        } else {
            div()
                .size(px(22.0))
                .rounded(px(5.0))
                .bg(theme.accent)
                .flex()
                .items_center()
                .justify_center()
                .text_size(sp(11.0))
                .font_weight(FontWeight::BOLD)
                .text_color(theme.on_inverse)
                .child(
                    label
                        .chars()
                        .next()
                        .unwrap_or('P')
                        .to_uppercase()
                        .to_string(),
                )
                .flex_none()
                .into_any_element()
        };

        let insulator = cx.entity().downgrade();
        let menu = self.menu_handle(format!("project-{group_key}"), cx);
        let project_id = match group {
            SidebarGroup::Project(project_id) => Some(project_id),
            SidebarGroup::Updated(_) | SidebarGroup::Projectless | SidebarGroup::Status(_) => None,
        };
        let is_status_group = matches!(group, SidebarGroup::Status(_));

        let chat_count = if let Some(project_id) = project_id {
            self.state
                .sessions
                .iter()
                .filter(|session| {
                    session.conversation_root_id.is_none()
                        && session.project_id == project_id
                        && session.has_started()
                })
                .count()
        } else if matches!(group, SidebarGroup::Projectless) {
            let projectless_root = crate::projectless::workspace_root();
            self.state
                .sessions
                .iter()
                .filter(|session| {
                    session.conversation_root_id.is_none()
                        && (session.has_started()
                            || self.state.selected_session == Some(session.id))
                        && self.state.projects.iter().any(|project| {
                            project.id == session.project_id
                                && sidebar_project_is_projectless(
                                    project,
                                    projectless_root.as_deref(),
                                )
                        })
                })
                .count()
        } else if let SidebarGroup::Status(status) = group {
            self.state
                .sessions
                .iter()
                .filter(|session| {
                    session.conversation_root_id.is_none()
                        && session.chat_status == status
                        && session.has_started()
                })
                .count()
        } else {
            0
        };

        let status_icon_el = if let SidebarGroup::Status(status) = group {
            Some(icon(status.icon(), 14.0, chat_status_color(&theme, status)).into_any_element())
        } else {
            None
        };
        let status_chevron = if let SidebarGroup::Status(_) = group {
            Some(
                div()
                    .flex_none()
                    .opacity(0.0)
                    .group_hover(group_name.clone(), |style| style.opacity(1.0))
                    .child(icon(
                        if collapsed {
                            "icons/chevron-right.svg"
                        } else {
                            "icons/chevron-down.svg"
                        },
                        12.0,
                        theme.text_secondary,
                    )),
            )
        } else {
            None
        };

        let header = session_group_header(&theme)
            .h(px(38.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(theme.text)
            .id(SharedString::from(format!(
                "sidebar-group-toggle-{group_key}"
            )))
            .track_focus(&header_focus)
            .tab_index(0)
            .tab_group()
            .tab_stop(true)
            .group(group_name)
            .relative()
            .w_full()
            .rounded(px(7.0))
            .border_1()
            .border_color(gpui::transparent_black())
            .cursor_default()
            .focus_visible(|style| style.border_1().border_color(theme.accent))
            .hover(|style| style.bg(theme.sidebar_item_background))
            .active(|style| style.bg(theme.overlay_strong))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(24.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(show_folder_icon, |element| element.child(project_icon_el))
                    .when_some(status_icon_el, |element, status_icon| {
                        element.child(status_icon)
                    })
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(sp(14.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(label),
                    )
                    .when(
                        (show_folder_icon || is_status_group) && chat_count > 0,
                        |element| {
                            element.child(
                                div()
                                    .flex_none()
                                    .text_size(sp(14.0))
                                    .font_weight(FontWeight::NORMAL)
                                    .text_color(theme.text_tertiary)
                                    .child(format!("{chat_count}")),
                            )
                        },
                    )
                    .child(div().flex_1())
                    .when_some(status_chevron, |element, chevron| element.child(chevron)),
            )
            .when_some(compose, |element, compose| element.child(compose))
            .when_some(
                if let SidebarGroup::Status(target_status) = group {
                    Some(target_status)
                } else {
                    None
                },
                |element, target_status| {
                    let target_status_color = chat_status_color(&theme, target_status);
                    element
                        .drag_over::<DraggedSession>(move |style, dragged, _window, _cx| {
                            if dragged.status != target_status {
                                style
                                    .bg(target_status_color.opacity(0.18))
                                    .border_1()
                                    .border_color(target_status_color.opacity(0.7))
                            } else {
                                style
                            }
                        })
                        .on_drop(
                            cx.listener(move |this, dragged: &DraggedSession, _window, cx| {
                                if dragged.status != target_status {
                                    this.set_session_chat_status(
                                        dragged.session_id,
                                        target_status,
                                        cx,
                                    );
                                    crate::audio::play_arrival();
                                    this.set_sidebar_group_expanded(
                                        SidebarGroup::Status(target_status),
                                        true,
                                        cx,
                                    );
                                }
                            }),
                        )
                },
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.toggle_sidebar_group(group, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                match event.keystroke.key.as_str() {
                    "enter" | "space" => {
                        this.toggle_sidebar_group(group, cx);
                        cx.stop_propagation();
                    }
                    "left" if !collapsed => {
                        this.set_sidebar_group_expanded(group, false, cx);
                        cx.stop_propagation();
                    }
                    "right" if collapsed => {
                        this.set_sidebar_group_expanded(group, true, cx);
                        cx.stop_propagation();
                    }
                    _ => {}
                }
            }));

        let header_element = if let Some(project_id) = project_id {
            let project_path = self
                .state
                .projects
                .iter()
                .find(|p| p.id == project_id)
                .map(|p| p.path.clone());
            context_menu(
                div().w_full().child(header),
                SharedString::from(format!("project-menu-{project_id}")),
                &menu,
                move |_| {
                    let rename_insulator = insulator.clone();
                    let remove_insulator = insulator.clone();
                    let mut items = Vec::new();

                    if let Some(ref path) = project_path {
                        if let Some(github_url) = github_url_for_project(path) {
                            items.push(
                                MenuItem::new(tr!("project.open_in_github"), move |_, cx| {
                                    cx.open_url(&github_url);
                                })
                                .icon("icons/github.svg"),
                            );
                            items.push(MenuItem::Separator);
                        }
                    }

                    items.push(MenuItem::new(tr!("common.rename"), move |window, cx| {
                        let _ = rename_insulator.update(cx, |insulator, cx| {
                            insulator.open_rename_project_dialog(project_id, window, cx);
                        });
                    }));
                    items.push(MenuItem::Separator);
                    items.push(MenuItem::new(tr!("common.remove"), move |_, cx| {
                        let _ = remove_insulator.update(cx, |insulator, cx| {
                            insulator.remove_project(project_id, cx);
                        });
                    }));

                    items
                },
            )
        } else {
            div().w_full().child(header).into_any_element()
        };

        div()
            .w_full()
            .pb(px(SIDEBAR_GROUP_HEADER_BOTTOM_GAP))
            .child(header_element)
    }

    fn open_new_task_for_sidebar_group(
        &mut self,
        group: SidebarGroup,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.settings_page = None;
        match group {
            SidebarGroup::Project(project_id) => {
                self.sidebar_expanded_groups.insert(group);
                self.create_session_for(project_id, self.state.last_provider, cx);
            }
            SidebarGroup::Projectless => {
                self.sidebar_expanded_groups.insert(group);
                self.create_projectless_session(cx);
            }
            SidebarGroup::Updated(_) | SidebarGroup::Status(_) => return,
        }
        let focus_handle = self.composer_focus(cx);
        window.focus(&focus_handle, cx);
    }

    fn render_sidebar_show_more(&self, group: SidebarGroup, cx: &mut Context<Self>) -> Div {
        let theme = Theme::current(cx);
        let group_key = group.element_key();
        let focus = self
            .sidebar_show_more_focuses
            .borrow_mut()
            .entry(group)
            .or_insert_with(|| cx.focus_handle())
            .clone();
        let button = div()
            .id(SharedString::from(format!("sidebar-show-more-{group_key}")))
            .track_focus(&focus)
            .tab_index(0)
            .tab_stop(true)
            .flex_none()
            .cursor_default()
            .text_size(sp(12.5))
            .text_color(theme.text_tertiary)
            .focus_visible(|style| style.text_color(theme.text))
            .hover(|style| style.text_color(theme.text))
            .child(tr!("sidebar.show_more"))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.show_more_project_sessions(group, cx);
            }))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.show_more_project_sessions(group, cx);
                    cx.stop_propagation();
                }
            }));

        div()
            .relative()
            .w_full()
            .h(px(SIDEBAR_SHOW_MORE_ROW_HEIGHT))
            .when(
                self.state.sidebar_grouping != SidebarGrouping::Project,
                |element| element.pl(px(SIDEBAR_GROUP_CHILD_PADDING)),
            )
            .when(
                self.state.sidebar_grouping == SidebarGrouping::Project,
                |element| element.pl(px(8.0)),
            )
            .flex()
            .items_center()
            .child(button)
            .when(
                self.state.sidebar_grouping != SidebarGrouping::Project,
                |element| {
                    element.child(
                        div()
                            .absolute()
                            .left(px(SIDEBAR_GROUP_GUIDE_X))
                            .top_0()
                            .w(px(SIDEBAR_GROUP_CHILD_PADDING
                                - SIDEBAR_GROUP_GUIDE_X
                                - 4.0))
                            .h(px(15.0))
                            .border_l_1()
                            .border_b_1()
                            .rounded_bl(px(4.0))
                            .border_color(theme.border),
                    )
                },
            )
    }

    fn show_more_project_sessions(&mut self, group: SidebarGroup, cx: &mut Context<Self>) {
        let revealed = self.sidebar_project_reveal_counts.entry(group).or_default();
        *revealed = revealed.saturating_add(SIDEBAR_PROJECT_REVEAL_BATCH);
        self.sidebar_rows_fingerprint.set(None);
        cx.notify();
    }

    fn toggle_sidebar_group(&mut self, group: SidebarGroup, cx: &mut Context<Self>) {
        let expanded = !self.sidebar_expanded_groups.contains(&group);
        self.set_sidebar_group_expanded(group, expanded, cx);
    }

    pub(super) fn collapse_all_sidebar_groups(&mut self, cx: &mut Context<Self>) {
        self.sidebar_expanded_groups.clear();
        self.sidebar_project_reveal_counts.clear();
        self.sidebar_rows_fingerprint.set(None);
        cx.notify();
    }

    fn set_sidebar_group_expanded(
        &mut self,
        group: SidebarGroup,
        expanded: bool,
        cx: &mut Context<Self>,
    ) {
        let changed = if expanded {
            self.sidebar_expanded_groups.insert(group)
        } else {
            self.sidebar_expanded_groups.remove(&group)
        };
        let reveal_reset = !expanded && self.sidebar_project_reveal_counts.remove(&group).is_some();
        if changed || reveal_reset {
            self.sidebar_rows_fingerprint.set(None);
            cx.notify();
        }
    }

    fn toggle_session_pinned(&mut self, session_id: Uuid, cx: &mut Context<Self>) {
        if self.pinned_session_ids.contains(&session_id) {
            self.pinned_session_ids.remove(&session_id);
        } else {
            self.pinned_session_ids.insert(session_id);
        }
        self.sidebar_rows_fingerprint.set(None);
        self.save();
        cx.notify();
    }

    fn set_sidebar_grouping(&mut self, grouping: SidebarGrouping, cx: &mut Context<Self>) {
        if self.state.sidebar_grouping == grouping {
            return;
        }
        self.state.sidebar_grouping = grouping;
        if grouping == SidebarGrouping::Status {
            self.sidebar_expanded_groups
                .insert(SidebarGroup::Status(ChatStatus::InProgress));
        }
        self.sidebar_rows_fingerprint.set(None);
        self.sidebar_branch_scan_fingerprint.set(None);
        self.sidebar_branch_scan_generation
            .set(self.sidebar_branch_scan_generation.get().wrapping_add(1));
        self.sidebar_list_state.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: Pixels::ZERO,
        });
        self.save();
        cx.notify();
    }

    fn set_sidebar_ordering(&mut self, ordering: SidebarOrdering, cx: &mut Context<Self>) {
        if self.state.sidebar_ordering == ordering {
            return;
        }
        self.state.sidebar_ordering = ordering;
        self.sidebar_rows_fingerprint.set(None);
        self.sidebar_list_state.scroll_to(ListOffset {
            item_ix: 0,
            offset_in_item: Pixels::ZERO,
        });
        self.save();
        cx.notify();
    }

    fn begin_session_rename(
        &mut self,
        session_id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(title) = self
            .state
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .map(localized_session_title)
        else {
            return;
        };

        self.session_rename = Some(session_id);
        self.session_rename_input.update(cx, |input, cx| {
            input.set_content(title, cx);
            input.select_all_text(cx);
        });
        let focus = self.session_rename_input.read(cx).focus();
        window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        cx.notify();
    }

    pub(super) fn commit_session_rename(&mut self, cx: &mut Context<Self>) {
        let Some(session_id) = self.session_rename.take() else {
            return;
        };
        let title = self
            .session_rename_input
            .read(cx)
            .content()
            .trim()
            .to_owned();
        let should_update = !title.is_empty()
            && self
                .state
                .sessions
                .iter()
                .find(|session| session.id == session_id)
                .is_some_and(|session| session.title != title);
        if should_update
            && self
                .state
                .session_mut(session_id)
                .is_some_and(|session| session.set_title(&title))
        {
            self.save();
        }
        cx.notify();
    }

    fn cancel_session_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.session_rename.take().is_none() {
            return;
        }
        let focus = self.composer_focus(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn render_sidebar_session_item(
        &self,
        session_id: Uuid,
        pinned_copy: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let theme = Theme::current(cx);
        let Some(session) = self
            .state
            .sessions
            .iter()
            .find(|session| session.id == session_id)
        else {
            return div().into_any_element();
        };
        let selected = sidebar_session_selected(
            self.state.selected_session,
            self.pending_session_activation
                .map(|pending| pending.session_id),
            session_id,
        );
        let working = matches!(
            session.status,
            SessionStatus::Connecting | SessionStatus::Working
        );
        let branch_label = persisted_sidebar_branch_label(&session.workspace);
        let title_str = branch_label
            .map(|branch| SharedString::from(branch.to_owned()))
            .unwrap_or_else(|| SharedString::from(localized_session_title(session)));
        let is_flat = self.state.sidebar_grouping == SidebarGrouping::Chats;
        let left_margin = if is_flat { 4.0 } else { 12.0 };
        let right_margin = 4.0;
        let is_status_grouping = self.state.sidebar_grouping == SidebarGrouping::Status;
        let providers = sidebar_session_providers(session);
        let icon_element = if working {
            dot_matrix_loader(theme.text_secondary, 18.0)
        } else if is_status_grouping {
            let project = self
                .state
                .projects
                .iter()
                .find(|p| p.id == session.project_id);
            let avatar_path = crate::platform::local_user_github_avatar_path();
            if let Some(path) = avatar_path.as_ref() {
                img(path.clone())
                    .size(px(18.0))
                    .rounded(px(4.0))
                    .flex_none()
                    .into_any_element()
            } else {
                let display_char = project
                    .map(Project::display_name)
                    .unwrap_or_else(|| "W".to_string())
                    .chars()
                    .next()
                    .unwrap_or('W')
                    .to_uppercase()
                    .to_string();
                div()
                    .size(px(18.0))
                    .rounded(px(4.0))
                    .bg(theme.accent)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(sp(10.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(theme.on_inverse)
                    .child(display_char)
                    .flex_none()
                    .into_any_element()
            }
        } else {
            sidebar_provider_mark(&theme, &providers).into_any_element()
        };
        let rename_input =
            (self.session_rename == Some(session_id)).then(|| self.session_rename_input.clone());
        let renaming = rename_input.is_some();
        let title = if let Some(rename_input) = rename_input {
            div()
                .id(SharedString::from(format!(
                    "session-rename-field-{session_id}"
                )))
                .key_context(SESSION_RENAME_PARENT_CONTEXT)
                .on_action(cx.listener(|this, _: &CancelSessionRename, window, cx| {
                    this.cancel_session_rename(window, cx);
                }))
                .h(px(18.0))
                .flex_1()
                .min_w_0()
                .px(px(4.0))
                .rounded(px(4.0))
                .border_1()
                .border_color(theme.accent)
                .bg(theme.inset)
                .flex()
                .items_center()
                .text_size(sp(13.0))
                .text_color(theme.text)
                .child(rename_input)
                .into_any_element()
        } else {
            div()
                .flex_1()
                .min_w_0()
                .truncate()
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
                .child(title_str.clone())
                .into_any_element()
        };
        let insulator = cx.entity().downgrade();
        let current_status = session.chat_status;
        let menu = self.menu_handle(format!("session-{session_id}"), cx);
        let row_focus = menu.trigger_focus_handle().clone();
        let keyboard_menu = menu.clone();
        let dragged = DraggedSession {
            session_id,
            status: current_status,
            title: title_str.clone(),
        };
        let row = div()
            .id(SharedString::from(if pinned_copy {
                format!("pinned-session-{}", session.id)
            } else {
                format!("session-{}", session.id)
            }))
            .ml(px(left_margin))
            .mr(px(right_margin))
            .h(px(SIDEBAR_SESSION_CARD_HEIGHT))
            .px(px(8.0))
            .flex()
            .items_center()
            .rounded(px(7.0))
            .border_1()
            .border_color(gpui::transparent_black())
            .cursor_default()
            .when(selected, |element| {
                element.bg(theme.sidebar_item_background)
            })
            .hover(|element| element.bg(theme.sidebar_item_background))
            .active(|element| element.bg(theme.overlay_strong))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .overflow_hidden()
                    .child(icon_element)
                    .when(is_status_grouping && branch_label.is_some(), |element| {
                        element.child(icon("icons/fork.svg", 12.0, theme.text_secondary))
                    })
                    .child(title)
                    .when(session.status == SessionStatus::Background, |element| {
                        element.child(icon(
                            "icons/hourglass.svg",
                            12.0,
                            status_color(&theme, session.status),
                        ))
                    })
                    .when(session.status == SessionStatus::Waiting, |element| {
                        element.child(icon(
                            "icons/alert.svg",
                            12.0,
                            status_color(&theme, session.status),
                        ))
                    })
                    .when(session.status == SessionStatus::Failed, |element| {
                        element.child(icon(
                            "icons/x.svg",
                            12.0,
                            status_color(&theme, session.status),
                        ))
                    }),
            )
            .when(!renaming, |element| {
                let dragged_payload = dragged.clone();
                element
                    .track_focus(&row_focus)
                    .tab_index(0)
                    .focus_visible(|style| style.border_1().border_color(theme.accent))
                    .on_drag(dragged_payload, move |dragged, _, _window, cx| {
                        cx.new(|_| DraggedSessionPreview {
                            title: dragged.title.clone(),
                            status: dragged.status,
                        })
                    })
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        let key = event.keystroke.key.as_str();
                        if matches!(key, "enter" | "space") {
                            this.select_session(session_id, cx);
                            cx.stop_propagation();
                        } else if key == "f10" && event.keystroke.modifiers.shift {
                            keyboard_menu.open_context_menu(window, cx);
                            cx.stop_propagation();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_session(session_id, cx);
                    }))
            })
            .when(is_status_grouping && !pinned_copy, |element| {
                let target_status = current_status;
                let target_status_color = chat_status_color(&theme, target_status);
                element
                    .drag_over::<DraggedSession>(move |style, dragged, _window, _cx| {
                        if dragged.status != target_status && dragged.session_id != session_id {
                            style
                                .bg(target_status_color.opacity(0.18))
                                .border_1()
                                .border_color(target_status_color.opacity(0.7))
                        } else {
                            style
                        }
                    })
                    .on_drop(
                        cx.listener(move |this, dragged: &DraggedSession, _window, cx| {
                            if dragged.status != target_status {
                                this.set_session_chat_status(dragged.session_id, target_status, cx);
                                crate::audio::play_arrival();
                                this.set_sidebar_group_expanded(
                                    SidebarGroup::Status(target_status),
                                    true,
                                    cx,
                                );
                            }
                        }),
                    )
            });
        let is_pinned = self.pinned_session_ids.contains(&session_id);
        let row = if renaming {
            div()
                .w_full()
                .child(row)
                .on_mouse_down_out(cx.listener(move |this, _, _, cx| {
                    if this.session_rename == Some(session_id) {
                        this.commit_session_rename(cx);
                    }
                }))
                .into_any_element()
        } else {
            context_menu(
                div().w_full().child(row),
                SharedString::from(if pinned_copy {
                    format!("pinned-session-menu-{session_id}")
                } else {
                    format!("session-menu-{session_id}")
                }),
                &menu,
                move |cx| {
                    let theme = Theme::current(cx);
                    let status_insulator = insulator.clone();
                    let pin_insulator = insulator.clone();
                    let rename_insulator = insulator.clone();
                    let remove_insulator = insulator.clone();

                    let set_status_submenu = MenuItem::submenu("Set status", move |cx| {
                        let theme = Theme::current(cx);
                        ChatStatus::MENU_ORDER
                            .iter()
                            .map(|&status| {
                                let status_insulator = status_insulator.clone();
                                MenuItem::new(status.label(), move |_window, cx| {
                                    let _ = status_insulator.update(cx, |insulator, cx| {
                                        insulator.set_session_chat_status(session_id, status, cx);
                                    });
                                })
                                .icon(status.icon())
                                .icon_color(chat_status_color(&theme, status))
                                .selected(current_status == status)
                            })
                            .collect()
                    })
                    .icon(current_status.icon())
                    .icon_color(chat_status_color(&theme, current_status));

                    vec![
                        MenuItem::new(if is_pinned { "Unpin" } else { "Pin" }, move |_, cx| {
                            let _ = pin_insulator.update(cx, |insulator, cx| {
                                insulator.toggle_session_pinned(session_id, cx);
                            });
                        })
                        .icon(if is_pinned {
                            "icons/star-filled.svg"
                        } else {
                            "icons/star.svg"
                        }),
                        set_status_submenu,
                        MenuItem::new(tr!("common.rename"), move |window, cx| {
                            let _ = rename_insulator.update(cx, |insulator, cx| {
                                insulator.open_rename_session_dialog(session_id, window, cx);
                            });
                        })
                        .icon("icons/pencil.svg"),
                        MenuItem::Separator,
                        MenuItem::new(tr!("common.remove"), move |_, cx| {
                            let _ = remove_insulator.update(cx, |insulator, cx| {
                                insulator.remove_session(session_id, cx)
                            });
                        })
                        .icon("icons/trash.svg"),
                    ]
                },
            )
        };

        div()
            .relative()
            .w_full()
            .pb(px(SIDEBAR_SESSION_ROW_GAP))
            .child(row)
            .into_any_element()
    }

    // ── Header ─────────────────────────────────────────────────────────────

    pub(super) fn render_header(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let theme = Theme::current(cx);
        let session = self.selected_session();
        let title = if self.pull_requests_open {
            Some("Pull requests".to_owned())
        } else {
            session.map(localized_session_title)
        };
        let agent_preset_label = session
            .filter(|session| session.provider == ProviderKind::DeepSeek && session.has_started())
            .and_then(|session| self.agent_preset_label_for_session(session));
        let left_window_controls = (!self.sidebar_visible)
            .then(|| {
                self.render_client_window_controls(
                    super::window_chrome::WindowControlSide::Left,
                    window,
                    cx,
                )
            })
            .flatten();
        let right_window_controls = (!self.right_panel_visible)
            .then(|| {
                self.render_client_window_controls(
                    super::window_chrome::WindowControlSide::Right,
                    window,
                    cx,
                )
            })
            .flatten();
        let show_tabs = (self.main_tabs_open || !self.main_tabs.is_empty())
            && self.selected_session().is_some();
        div()
            .id("window-header")
            .h(px(48.0))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(theme.border)
            .children(left_window_controls)
            // The header starts where the sidebar ends, so until the sidebar
            // is wide enough to host the traffic lights itself the header has
            // to clear them. Steady state with the sidebar open adds nothing;
            // a sidebar sliding in shrinks the inset as it takes the lights
            // over, which is what keeps the title from passing under them.
            .pl(if self.sidebar_visible {
                px(14.0 + (TRAFFIC_LIGHT_CLEARANCE - self.sidebar_rendered_width).max(0.0))
            } else {
                px(0.0)
            })
            .pr(px(14.0))
            .when(!self.sidebar_visible, |element| {
                element
                    .child(
                        self.window_drag_region(
                            div()
                                .id("header-traffic-light-drag-region")
                                .w(px(TRAFFIC_LIGHT_CLEARANCE - 8.0))
                                .h_full()
                                .flex_none(),
                            cx,
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(self.render_sidebar_toggle(cx))
                            .when(self.state.show_resource_usage, |bar| {
                                bar.child(self.render_resource_usage_button(cx))
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(2.0))
                                    .child(self.render_history_button(
                                        "navigate-back",
                                        "icons/arrow-left.svg",
                                        !self.session_navigation.back.is_empty(),
                                        true,
                                        cx,
                                    ))
                                    .child(self.render_history_button(
                                        "navigate-forward",
                                        "icons/arrow-right.svg",
                                        !self.session_navigation.forward.is_empty(),
                                        false,
                                        cx,
                                    )),
                            ),
                    )
            })
            .when(show_tabs, |element| {
                element.child(self.render_session_tabs(cx))
            })
            .when(!show_tabs, |element| {
                element.children(title.map(|title| {
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(sp(13.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .child(SharedString::from(title))
                }))
            })
            .children(agent_preset_label.map(|label| {
                div()
                    .h(px(22.0))
                    .max_w(px(180.0))
                    .px(px(6.0))
                    .rounded(px(6.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .bg(theme.overlay)
                    .text_size(sp(12.5))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(icon("icons/bot.svg", 10.5, theme.text_tertiary))
                    .child(div().min_w_0().truncate().child(SharedString::from(label)))
            }))
            .child(
                self.window_drag_region(
                    div().id("header-center-drag-region").h_full().flex_1(),
                    cx,
                ),
            )
            .child(self.render_background_work_summary(cx))
            .when(self.fps_counter_visible, |element| {
                element.child(self.render_fps_counter(cx))
            })
            .when(!self.right_panel_visible, |element| {
                element.child(self.render_right_panel_toggle(cx))
            })
            .children(right_window_controls)
    }

    pub(super) fn render_session_tabs(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let theme = Theme::current(cx);
        let insulator = cx.entity().downgrade();
        let mut tabs = Vec::new();
        let current_project_id = self.selected_project().map(|project| project.id);
        for (index, tab) in self.main_tabs.iter().enumerate() {
            match tab {
                MainTab::Chat(session_id) => {
                    let session_id = *session_id;
                    let Some(session) = self
                        .state
                        .sessions
                        .iter()
                        .find(|session| session.id == session_id)
                    else {
                        continue;
                    };
                    if let Some(project_id) = current_project_id {
                        if session.project_id != project_id {
                            continue;
                        }
                    }
                    let selected = !self.active_main_review_tab
                        && self.active_main_file_tab.is_none()
                        && self.state.selected_session == Some(session_id);
                    let provider = session.provider;
                    let working = matches!(
                        session.status,
                        SessionStatus::Connecting | SessionStatus::Working
                    );
                    let tab_mark = if working {
                        dot_matrix_loader(theme.accent, 12.0)
                    } else {
                        provider_mark(&theme, provider, 12.5, provider_color(&theme, provider))
                            .into_any_element()
                    };
                    let tab_insulator = insulator.clone();
                    let close_insulator = insulator.clone();
                    let middle_close_insulator = insulator.clone();
                    let tab_group = format!("main-chat-tab-grp-{session_id}");

                    tabs.push(
                        div()
                            .id(SharedString::from(format!("main-chat-tab-{session_id}")))
                            .group(tab_group.clone())
                            .tab_index(0)
                            .focus_visible(|element| element.border_color(theme.accent))
                            .h(px(28.0))
                            .min_w(px(72.0))
                            .max_w(px(190.0))
                            .pl(px(8.0))
                            .pr(px(5.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .text_size(sp(12.0))
                            .border_1()
                            .when(selected, |element| {
                                element
                                    .bg(theme.raised)
                                    .border_color(theme.border_strong)
                                    .text_color(theme.text)
                                    .font_weight(FontWeight::MEDIUM)
                                    .shadow_xs()
                            })
                            .when(!selected, |element| {
                                element
                                    .bg(gpui::transparent_black())
                                    .border_color(gpui::transparent_black())
                                    .text_color(theme.text_secondary)
                                    .font_weight(FontWeight::NORMAL)
                                    .hover(|element| {
                                        element
                                            .bg(theme.overlay)
                                            .border_color(theme.border)
                                            .text_color(theme.text)
                                    })
                            })
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_down(MouseButton::Middle, move |_, _, cx| {
                                cx.stop_propagation();
                                let _ = middle_close_insulator.update(cx, |insulator, cx| {
                                    insulator.close_main_chat_tab(session_id, cx);
                                });
                            })
                            .on_click(move |_, _, cx| {
                                let _ = tab_insulator.update(cx, |insulator, cx| {
                                    insulator.activate_main_tab_at_index(index, cx);
                                });
                            })
                            .child(tab_mark)
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .child(SharedString::from(localized_session_title(session))),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "main-chat-tab-close-{session_id}"
                                    )))
                                    .tab_index(0)
                                    .focus_visible(|el| el.opacity(1.0).bg(theme.overlay_strong))
                                    .size(px(16.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(3.0))
                                    .cursor_pointer()
                                    .when(selected, |el| el.opacity(0.55))
                                    .when(!selected, |el| el.opacity(0.0))
                                    .group_hover(tab_group.clone(), |style| style.opacity(1.0))
                                    .hover(|element| {
                                        element.bg(theme.overlay_strong).text_color(theme.text)
                                    })
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        let _ = close_insulator.update(cx, |insulator, cx| {
                                            insulator.close_main_chat_tab(session_id, cx);
                                        });
                                    })
                                    .child(icon("icons/x.svg", 9.0, theme.text_secondary)),
                            )
                            .into_any_element(),
                    );
                }
                MainTab::File(path) => {
                    let path = path.clone();
                    let path_for_close = path.clone();
                    let path_for_middle = path.clone();
                    let label = Path::new(&path)
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or(&path)
                        .to_owned();
                    let selected = !self.active_main_review_tab
                        && self.active_main_file_tab.as_deref() == Some(path.as_str());
                    let is_dirty = self.right_panel_file_is_dirty(&path);
                    let activate_insulator = insulator.clone();
                    let close_insulator = insulator.clone();
                    let middle_close_insulator = insulator.clone();
                    let tab_group = format!("main-file-tab-grp-{index}");

                    tabs.push(
                        div()
                            .id(SharedString::from(format!("main-file-tab-{index}")))
                            .group(tab_group.clone())
                            .tab_index(0)
                            .focus_visible(|element| element.border_color(theme.accent))
                            .h(px(28.0))
                            .min_w(px(72.0))
                            .max_w(px(190.0))
                            .pl(px(8.0))
                            .pr(px(5.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .text_size(sp(12.0))
                            .border_1()
                            .when(selected, |element| {
                                element
                                    .bg(theme.raised)
                                    .border_color(theme.border_strong)
                                    .text_color(theme.text)
                                    .font_weight(FontWeight::MEDIUM)
                                    .shadow_xs()
                            })
                            .when(!selected, |element| {
                                element
                                    .bg(gpui::transparent_black())
                                    .border_color(gpui::transparent_black())
                                    .text_color(theme.text_secondary)
                                    .font_weight(FontWeight::NORMAL)
                                    .hover(|element| {
                                        element
                                            .bg(theme.overlay)
                                            .border_color(theme.border)
                                            .text_color(theme.text)
                                    })
                            })
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_down(MouseButton::Middle, move |_, _, cx| {
                                cx.stop_propagation();
                                let _ = middle_close_insulator.update(cx, |insulator, cx| {
                                    insulator.close_main_file_tab(&path_for_middle, cx);
                                });
                            })
                            .on_click(move |_, _, cx| {
                                let _ = activate_insulator.update(cx, |insulator, cx| {
                                    insulator.activate_main_tab_at_index(index, cx);
                                });
                            })
                            .child(crate::ui::file_icon(file_icon_for_path(&path), 12.5))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .child(SharedString::from(label)),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("main-file-tab-close-{index}")))
                                    .tab_index(0)
                                    .focus_visible(|el| el.opacity(1.0).bg(theme.overlay_strong))
                                    .size(px(16.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(3.0))
                                    .cursor_pointer()
                                    .hover(|element| {
                                        element.bg(theme.overlay_strong).text_color(theme.text)
                                    })
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        let _ = close_insulator.update(cx, |insulator, cx| {
                                            insulator.close_main_file_tab(&path_for_close, cx);
                                        });
                                    })
                                    .when(is_dirty, |el| {
                                        el.child(
                                            div()
                                                .size(px(6.0))
                                                .rounded_full()
                                                .bg(theme.accent)
                                                .group_hover(tab_group.clone(), |style| {
                                                    style.invisible()
                                                }),
                                        )
                                        .child(
                                            div()
                                                .invisible()
                                                .group_hover(tab_group.clone(), |style| {
                                                    style.visible()
                                                })
                                                .child(icon("icons/x.svg", 9.0, theme.text_secondary)),
                                        )
                                    })
                                    .when(!is_dirty, |el| {
                                        el.when(selected, |el| el.opacity(0.55))
                                            .when(!selected, |el| el.opacity(0.0))
                                            .group_hover(tab_group.clone(), |style| {
                                                style.opacity(1.0)
                                            })
                                            .child(icon("icons/x.svg", 9.0, theme.text_secondary))
                                    }),
                            )
                            .into_any_element(),
                    );
                }
                MainTab::Review => {
                    let selected = self.active_main_review_tab;
                    let activate_insulator = insulator.clone();
                    let close_insulator = insulator.clone();
                    let middle_close_insulator = insulator.clone();
                    let tab_group = format!("main-review-tab-grp-{index}");

                    tabs.push(
                        div()
                            .id(SharedString::from(format!("main-review-tab-{index}")))
                            .group(tab_group.clone())
                            .tab_index(0)
                            .focus_visible(|element| element.border_color(theme.accent))
                            .h(px(28.0))
                            .min_w(px(72.0))
                            .max_w(px(190.0))
                            .pl(px(8.0))
                            .pr(px(5.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .rounded(px(6.0))
                            .cursor_pointer()
                            .text_size(sp(12.0))
                            .border_1()
                            .when(selected, |element| {
                                element
                                    .bg(theme.raised)
                                    .border_color(theme.border_strong)
                                    .text_color(theme.text)
                                    .font_weight(FontWeight::MEDIUM)
                                    .shadow_xs()
                            })
                            .when(!selected, |element| {
                                element
                                    .bg(gpui::transparent_black())
                                    .border_color(gpui::transparent_black())
                                    .text_color(theme.text_secondary)
                                    .font_weight(FontWeight::NORMAL)
                                    .hover(|element| {
                                        element
                                            .bg(theme.overlay)
                                            .border_color(theme.border)
                                            .text_color(theme.text)
                                    })
                            })
                            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                cx.stop_propagation();
                            })
                            .on_mouse_down(MouseButton::Middle, move |_, _, cx| {
                                cx.stop_propagation();
                                let _ = middle_close_insulator.update(cx, |insulator, cx| {
                                    insulator.close_main_review_tab(cx);
                                });
                            })
                            .on_click(move |_, _, cx| {
                                let _ = activate_insulator.update(cx, |insulator, cx| {
                                    insulator.activate_main_tab_at_index(index, cx);
                                });
                            })
                            .child(icon(
                                "icons/file-diff.svg",
                                12.5,
                                if selected {
                                    theme.text_secondary
                                } else {
                                    theme.text_tertiary
                                },
                            ))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .truncate()
                                    .child(tr!("right_panel.diff")),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "main-review-tab-close-{index}"
                                    )))
                                    .tab_index(0)
                                    .focus_visible(|el| {
                                        el.opacity(1.0)
                                            .bg(theme.overlay_strong)
                                            .text_color(theme.text)
                                    })
                                    .size(px(16.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .rounded(px(3.0))
                                    .cursor_pointer()
                                    .when(selected, |el| el.opacity(0.55))
                                    .when(!selected, |el| el.opacity(0.0))
                                    .group_hover(tab_group.clone(), |style| style.opacity(1.0))
                                    .hover(|element| {
                                        element.bg(theme.overlay_strong).text_color(theme.text)
                                    })
                                    .on_mouse_down(MouseButton::Left, |_, _, cx| {
                                        cx.stop_propagation();
                                    })
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        let _ = close_insulator.update(cx, |insulator, cx| {
                                            insulator.close_main_review_tab(cx);
                                        });
                                    })
                                    .child(icon("icons/x.svg", 9.0, theme.text_secondary)),
                            )
                            .into_any_element(),
                    );
                }
            }
        }

        let new_tab_insulator = cx.entity().downgrade();
        let new_tab = div()
            .id("session-tabs-new")
            .size(px(26.0))
            .ml(px(3.0))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .cursor_pointer()
            .hover(|element| element.bg(theme.overlay))
            .active(|element| element.bg(theme.overlay_strong))
            .tooltip(|window, cx| Tooltip::new(tr!("session.new_task")).build(window, cx))
            .on_mouse_down(MouseButton::Left, |_, _, cx| {
                cx.stop_propagation();
            })
            .on_click(move |_, window, cx| {
                let _ = new_tab_insulator.update(cx, |insulator, cx| {
                    insulator.main_tabs_open = true;
                    insulator.active_main_file_tab = None;
                    insulator.active_main_review_tab = false;
                    if let Some(session_id) = insulator.create_new_chat_tab(cx) {
                        let tab = MainTab::Chat(session_id);
                        if !insulator.main_tabs.contains(&tab) {
                            insulator.main_tabs.push(tab);
                        }
                        let index = insulator.main_tabs.len().saturating_sub(1);
                        insulator.main_tabs_scroll_handle.scroll_to_item(index);
                    }
                    let focus_handle = insulator.composer_focus(cx);
                    window.focus(&focus_handle, cx);
                    insulator.save();
                    cx.notify();
                });
            })
            .child(icon("icons/plus.svg", 12.0, theme.text_secondary));

        div()
            .id("session-tabs")
            .h_full()
            .min_w_0()
            .flex_shrink(1.0)
            .flex()
            .items_center()
            .child(
                div()
                    .id("session-tabs-wrapper")
                    .relative()
                    .min_w_0()
                    .flex_shrink(1.0)
                    .h_full()
                    .flex()
                    .items_center()
                    .child(
                        h_flex()
                            .id("session-tabs-scroll")
                            .h_full()
                            .items_center()
                            .overflow_x_scroll()
                            .track_scroll(&self.main_tabs_scroll_handle)
                            .on_scroll_wheel(cx.listener(|this, _, _, cx| {
                                contain_horizontal_scroll(&this.main_tabs_scroll_handle, cx);
                            }))
                            .gap(px(3.0))
                            .children(tabs),
                    )
                    .child(scrollbar::horizontal(
                        &self.main_tabs_scroll_handle,
                        &self.main_tabs_scrollbar,
                    )),
            )
            .child(new_tab)
    }

    // ── Empty states ───────────────────────────────────────────────────────

    pub(super) fn render_home_screen(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::current(cx);
        let logo = if theme.is_dark {
            crate::assets::home_logo()
        } else {
            crate::assets::light_home_logo()
        };
        div()
            .id("home-screen")
            .size_full()
            .min_w_0()
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .p(px(20.0))
            .child(
                div()
                    .w_full()
                    .max_w(px(640.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .child(
                        img(logo)
                            .w(px(360.0))
                            .h(px(123.0))
                            .max_w_full()
                            .mb(px(28.0))
                            .object_fit(ObjectFit::Contain),
                    )
                    .child(
                        div()
                            .w_full()
                            .flex()
                            .flex_wrap()
                            .items_center()
                            .justify_center()
                            .gap(px(12.0))
                            .child(
                                div()
                                    .id("home-card-open-project")
                                    .track_focus(&self.onboarding_add_project_focus)
                                    .tab_index(0)
                                    .w(px(200.0))
                                    .h(px(116.0))
                                    .p(px(16.0))
                                    .rounded(px(10.0))
                                    .border_1()
                                    .border_color(theme.border_strong)
                                    .bg(theme.raised)
                                    .cursor_pointer()
                                    .focus_visible(|s| s.border_color(theme.accent))
                                    .hover(|s| {
                                        s.bg(theme.composer).border_color(theme.text_secondary)
                                    })
                                    .flex()
                                    .flex_col()
                                    .justify_between()
                                    .child(div().flex().items_start().child(icon(
                                        "icons/folder-outline.svg",
                                        20.0,
                                        theme.text,
                                    )))
                                    .child(
                                        div()
                                            .text_size(sp(14.0))
                                            .text_color(theme.text)
                                            .font_weight(FontWeight::NORMAL)
                                            .child("Open project"),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.add_project(cx);
                                    }))
                                    .on_key_down(cx.listener(
                                        |this, event: &KeyDownEvent, _, cx| {
                                            if matches!(
                                                event.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) {
                                                this.add_project(cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    )),
                            )
                            .child(
                                div()
                                    .id("home-card-open-github")
                                    .track_focus(&self.onboarding_github_project_focus)
                                    .tab_index(1)
                                    .w(px(200.0))
                                    .h(px(116.0))
                                    .p(px(16.0))
                                    .rounded(px(10.0))
                                    .border_1()
                                    .border_color(theme.border_strong)
                                    .bg(theme.raised)
                                    .cursor_pointer()
                                    .focus_visible(|s| s.border_color(theme.accent))
                                    .hover(|s| {
                                        s.bg(theme.composer).border_color(theme.text_secondary)
                                    })
                                    .flex()
                                    .flex_col()
                                    .justify_between()
                                    .child(div().flex().items_start().child(icon(
                                        "icons/globe.svg",
                                        20.0,
                                        theme.text,
                                    )))
                                    .child(
                                        div()
                                            .text_size(sp(14.0))
                                            .text_color(theme.text)
                                            .font_weight(FontWeight::NORMAL)
                                            .child("Open GitHub project"),
                                    )
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.open_github_project_dialog(window, cx);
                                    }))
                                    .on_key_down(cx.listener(
                                        |this, event: &KeyDownEvent, window, cx| {
                                            if matches!(
                                                event.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) {
                                                this.open_github_project_dialog(window, cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    )),
                            )
                            .child(
                                div()
                                    .id("home-card-quick-start")
                                    .track_focus(&self.onboarding_projectless_focus)
                                    .tab_index(2)
                                    .w(px(200.0))
                                    .h(px(116.0))
                                    .p(px(16.0))
                                    .rounded(px(10.0))
                                    .border_1()
                                    .border_color(theme.border_strong)
                                    .bg(theme.raised)
                                    .cursor_pointer()
                                    .focus_visible(|s| s.border_color(theme.accent))
                                    .hover(|s| {
                                        s.bg(theme.composer).border_color(theme.text_secondary)
                                    })
                                    .flex()
                                    .flex_col()
                                    .justify_between()
                                    .child(div().flex().items_start().child(icon(
                                        "icons/folder-plus.svg",
                                        20.0,
                                        theme.text,
                                    )))
                                    .child(
                                        div()
                                            .text_size(sp(14.0))
                                            .text_color(theme.text)
                                            .font_weight(FontWeight::NORMAL)
                                            .child("Quick start"),
                                    )
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.create_projectless_session(cx);
                                    }))
                                    .on_key_down(cx.listener(
                                        |this, event: &KeyDownEvent, _, cx| {
                                            if matches!(
                                                event.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) {
                                                this.create_projectless_session(cx);
                                                cx.stop_propagation();
                                            }
                                        },
                                    )),
                            ),
                    ),
            )
            .into_any_element()
    }

    pub(super) fn render_empty_state(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::current(cx);
        if self.selected_project().is_none() || self.selected_session().is_none() {
            return self.render_home_screen(cx);
        }
        let selected_project_id = self.state.selected_project;
        let projectless_selected = self.selected_project().is_some_and(Project::is_projectless);
        let project_name = self
            .selected_project()
            .map(|project| {
                if project.is_projectless() {
                    tr!("project.without_a_project")
                } else {
                    project.display_name()
                }
            })
            .unwrap_or_else(|| tr!("project.your_project"));
        let project_options = self
            .state
            .projects
            .iter()
            .filter(|project| !project.is_projectless())
            .filter(|project| Some(project.id) == selected_project_id)
            .chain(
                self.state
                    .projects
                    .iter()
                    .filter(|project| !project.is_projectless())
                    .filter(|project| Some(project.id) != selected_project_id),
            )
            .map(|project| (project.id, project.display_name()))
            .collect::<Vec<_>>();
        let weak = cx.entity().downgrade();
        let handle = self.menu_handle("empty-state-project", cx);
        let project_selector = dropdown_menu(
            ProjectNameSelector::new("empty-state-project", project_name)
                .selected(handle.is_open()),
            "empty-state-project-menu",
            &handle,
            MenuAlign::BelowLeft,
            move |_| {
                let mut items = project_options
                    .clone()
                    .into_iter()
                    .map(|(project_id, project_name)| {
                        let weak = weak.clone();
                        MenuItem::new(project_name, move |_, cx| {
                            if Some(project_id) == selected_project_id {
                                return;
                            }
                            let _ = weak.update(cx, |this, cx| this.select_project(project_id, cx));
                        })
                        .selected(Some(project_id) == selected_project_id)
                    })
                    .collect::<Vec<_>>();
                if !items.is_empty() {
                    items.push(MenuItem::Separator);
                }
                let add_project_weak = weak.clone();
                items.push(
                    MenuItem::new(tr!("project.new_project"), move |window, cx| {
                        let _ = add_project_weak
                            .update(cx, |this, cx| this.open_project_source_dialog(window, cx));
                    })
                    .icon("icons/folder-new.svg"),
                );
                let projectless_weak = weak.clone();
                items.push(
                    MenuItem::new(tr!("project.no_project"), move |_, cx| {
                        let _ = projectless_weak.update(cx, |this, cx| {
                            if !this.selected_project().is_some_and(Project::is_projectless) {
                                this.create_projectless_session(cx);
                            }
                        });
                    })
                    .icon("icons/sparkle.svg"),
                );
                items
            },
        );
        div()
            .flex_1()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .px(px(20.0))
            .pb(px(52.0))
            .child(
                img(crate::assets::insulator_mark())
                    .w(px(57.0))
                    .h(px(72.0))
                    .object_fit(ObjectFit::Contain),
            )
            .child(
                div()
                    .mt(px(16.0))
                    .max_w_full()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_center()
                    .text_center()
                    .text_size(sp(20.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text)
                    .when(projectless_selected, |element| {
                        element.child(tr_cow!("onboarding.what_should_we_build"))
                    })
                    .when(!projectless_selected, |element| {
                        element
                            .child(
                                div()
                                    .flex_none()
                                    .child(tr_cow!("onboarding.what_should_we_build_in")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .min_w_0()
                                    .child(project_selector)
                                    .child(tr_cow!("onboarding.question_mark")),
                            )
                    }),
            )
            .into_any_element()
    }
}

pub(super) fn localized_session_title(session: &AgentSession) -> String {
    let title = session.display_title();
    if title == AgentSession::DEFAULT_TITLE {
        tr!("session.new_task")
    } else {
        title.to_owned()
    }
}

fn sidebar_session_selected(
    selected_session: Option<Uuid>,
    pending_session: Option<Uuid>,
    session_id: Uuid,
) -> bool {
    pending_session.map_or(selected_session == Some(session_id), |pending| {
        pending == session_id
    })
}

/// Resolves the GitHub repository web URL for a given project directory, if it is a
/// Git repository configured with a remote pointing to GitHub.
pub fn github_url_for_project(path: &Path) -> Option<String> {
    // 1. Direct file resolution (fastest, no subprocess)
    if let Some(git_dir) = resolve_git_dir(path) {
        if let Some(url) = extract_github_url_from_git_dir(&git_dir) {
            return Some(url);
        }
    }

    // 2. Fallback to git CLI if git is installed and path is inside a repo
    git_remote_github_url_via_cli(path)
}

fn resolve_git_dir(project_path: &Path) -> Option<PathBuf> {
    let dot_git = project_path.join(".git");
    if dot_git.is_dir() {
        return Some(dot_git);
    }
    if dot_git.is_file() {
        if let Ok(content) = std::fs::read_to_string(&dot_git) {
            if let Some(line) = content.lines().next() {
                if let Some(rest) = line.strip_prefix("gitdir:") {
                    let gitdir_path = PathBuf::from(rest.trim());
                    if gitdir_path.is_absolute() {
                        return Some(gitdir_path);
                    } else {
                        return Some(project_path.join(gitdir_path));
                    }
                }
            }
        }
    }
    None
}

fn extract_github_url_from_git_dir(git_dir: &Path) -> Option<String> {
    let config_path = git_dir.join("config");
    if let Ok(config_text) = std::fs::read_to_string(config_path) {
        if let Some(url) = find_github_url_in_git_config(&config_text) {
            return Some(url);
        }
    }

    // If this is a worktree, its commondir points to the common repo git dir
    let commondir_file = git_dir.join("commondir");
    if let Ok(commondir_content) = std::fs::read_to_string(commondir_file) {
        let common_path = git_dir.join(commondir_content.trim());
        let config_path = common_path.join("config");
        if let Ok(config_text) = std::fs::read_to_string(config_path) {
            if let Some(url) = find_github_url_in_git_config(&config_text) {
                return Some(url);
            }
        }
    }

    None
}

fn find_github_url_in_git_config(config_text: &str) -> Option<String> {
    let mut current_remote: Option<String> = None;
    let mut origin_url: Option<String> = None;
    let mut upstream_url: Option<String> = None;
    let mut other_url: Option<String> = None;

    for line in config_text.lines() {
        let line = line.trim();
        if line.starts_with('[') && line.ends_with(']') {
            let section = line[1..line.len() - 1].trim();
            if let Some(rest) = section.strip_prefix("remote ") {
                let name = rest.trim_matches('"').trim();
                current_remote = Some(name.to_owned());
            } else {
                current_remote = None;
            }
        } else if let Some(ref remote_name) = current_remote {
            if let Some((key, val)) = line.split_once('=') {
                if key.trim() == "url" {
                    let val = val.trim();
                    if let Some(gh_url) = parse_github_remote_url(val) {
                        if remote_name == "origin" {
                            origin_url = Some(gh_url);
                        } else if remote_name == "upstream" {
                            upstream_url = Some(gh_url);
                        } else if other_url.is_none() {
                            other_url = Some(gh_url);
                        }
                    }
                }
            }
        }
    }

    origin_url.or(upstream_url).or(other_url)
}

fn git_remote_github_url_via_cli(path: &Path) -> Option<String> {
    // Check origin first
    if let Ok(output) = std::process::Command::new("git")
        .args(["remote", "get-url", "origin"])
        .current_dir(path)
        .output()
    {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            if let Some(url) = parse_github_remote_url(&raw) {
                return Some(url);
            }
        }
    }

    // Check upstream or other remotes
    if let Ok(output) = std::process::Command::new("git")
        .args(["config", "--get-regexp", r"^remote\..*\.url$"])
        .current_dir(path)
        .output()
    {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout);
            let mut upstream_url = None;
            let mut other_url = None;
            for line in raw.lines() {
                if let Some((key, val)) = line.split_once(' ') {
                    if let Some(gh_url) = parse_github_remote_url(val) {
                        if key.contains("origin") {
                            return Some(gh_url);
                        } else if key.contains("upstream") {
                            upstream_url = Some(gh_url);
                        } else if other_url.is_none() {
                            other_url = Some(gh_url);
                        }
                    }
                }
            }
            return upstream_url.or(other_url);
        }
    }

    None
}

/// Parse a Git remote URL (SSH, HTTPS, git://, etc.) pointing to GitHub into a web URL
/// like `https://github.com/owner/repo`.
pub fn parse_github_remote_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() {
        return None;
    }

    // SSH format: git@github.com:owner/repo(.git)
    if let Some(rest) = raw.strip_prefix("git@github.com:") {
        let repo_path = rest.trim_end_matches(".git").trim_matches('/');
        if !repo_path.is_empty() {
            return Some(format!("https://github.com/{repo_path}"));
        }
    }

    // ssh://git@github.com/owner/repo(.git) or ssh://github.com/owner/repo(.git)
    if let Some(rest) = raw
        .strip_prefix("ssh://git@github.com/")
        .or_else(|| raw.strip_prefix("ssh://github.com/"))
    {
        let repo_path = rest.trim_end_matches(".git").trim_matches('/');
        if !repo_path.is_empty() {
            return Some(format!("https://github.com/{repo_path}"));
        }
    }

    // https://, http://, git://
    for prefix in &[
        "https://github.com/",
        "http://github.com/",
        "git://github.com/",
    ] {
        if let Some(rest) = raw.strip_prefix(prefix) {
            let repo_path = rest.trim_end_matches(".git").trim_matches('/');
            if !repo_path.is_empty() {
                return Some(format!("https://github.com/{repo_path}"));
            }
        }
    }

    // General URL parser for any remaining variations
    if let Ok(url) = url::Url::parse(raw) {
        if url.host_str() == Some("github.com") {
            let path = url.path().trim_end_matches(".git").trim_matches('/');
            if !path.is_empty() {
                return Some(format!("https://github.com/{path}"));
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_sessions_by_calendar_period() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 12).unwrap();
        let cases = [
            ((2026, 8, 12), SessionDateGroup::Today),
            ((2026, 8, 11), SessionDateGroup::Yesterday),
            ((2026, 8, 10), SessionDateGroup::ThisWeek),
            ((2026, 8, 1), SessionDateGroup::ThisMonth),
            ((2026, 1, 1), SessionDateGroup::ThisYear),
            ((2025, 12, 31), SessionDateGroup::More),
        ];

        for ((year, month, day), expected) in cases {
            let session_date = NaiveDate::from_ymd_opt(year, month, day).unwrap();
            assert_eq!(session_date_group_for_dates(session_date, today), expected);
        }
    }

    #[test]
    fn future_sessions_stay_in_today() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 12).unwrap();
        let tomorrow = NaiveDate::from_ymd_opt(2026, 8, 13).unwrap();
        assert_eq!(
            session_date_group_for_dates(tomorrow, today),
            SessionDateGroup::Today
        );
    }

    #[test]
    fn workspace_always_shows_in_sidebar() {
        assert!(sidebar_project_should_show(0));
        assert!(sidebar_project_should_show(1));
    }

    #[test]
    fn collapsed_sidebar_group_keeps_only_its_header_and_spacer() {
        let sessions = [Uuid::from_u128(1), Uuid::from_u128(2)];
        let group = SidebarGroup::Updated(SessionDateGroup::Today);
        let mut expanded = Vec::new();
        append_sidebar_group_rows(&mut expanded, group, &sessions, false, false);
        assert_eq!(
            expanded,
            vec![
                SidebarRow::Header(group),
                SidebarRow::Session(sessions[0]),
                SidebarRow::Session(sessions[1]),
                SidebarRow::GroupSpacer,
            ]
        );

        let mut collapsed = Vec::new();
        append_sidebar_group_rows(&mut collapsed, group, &sessions, true, false);
        assert_eq!(
            collapsed,
            vec![SidebarRow::Header(group), SidebarRow::GroupSpacer,]
        );
    }

    #[test]
    fn hidden_project_sessions_keep_a_keyboard_reveal_row() {
        let group = SidebarGroup::Project(Uuid::from_u128(1));
        let mut expanded = Vec::new();
        append_sidebar_group_rows(&mut expanded, group, &[], false, true);
        assert_eq!(
            expanded,
            vec![
                SidebarRow::Header(group),
                SidebarRow::ShowMore(group),
                SidebarRow::GroupSpacer,
            ]
        );

        let mut collapsed = Vec::new();
        append_sidebar_group_rows(&mut collapsed, group, &[], true, true);
        assert_eq!(
            collapsed,
            vec![SidebarRow::Header(group), SidebarRow::GroupSpacer]
        );
    }

    #[test]
    fn project_sessions_reveal_older_history_in_thirty_item_batches() {
        let sessions = (1..=36).map(Uuid::from_u128).collect::<Vec<_>>();
        let recent_cutoff = 100;
        let timestamps = sessions
            .iter()
            .enumerate()
            .map(|(index, session_id)| {
                (
                    *session_id,
                    if index == 0 {
                        recent_cutoff
                    } else {
                        recent_cutoff - 1
                    },
                )
            })
            .collect::<HashMap<_, _>>();

        let (initial, show_more) =
            visible_project_sessions(&sessions, &timestamps, recent_cutoff, 0);
        assert_eq!(initial, vec![sessions[0]]);
        assert!(show_more);

        let (first_batch, show_more) = visible_project_sessions(
            &sessions,
            &timestamps,
            recent_cutoff,
            SIDEBAR_PROJECT_REVEAL_BATCH,
        );
        assert_eq!(first_batch, sessions[..31]);
        assert!(show_more);

        let (all_sessions, show_more) = visible_project_sessions(
            &sessions,
            &timestamps,
            recent_cutoff,
            SIDEBAR_PROJECT_REVEAL_BATCH * 2,
        );
        assert_eq!(all_sessions, sessions);
        assert!(!show_more);
    }

    #[test]
    fn sidebar_recency_uses_last_reply_with_creation_fallback() {
        let project_id = Uuid::new_v4();
        let mut renamed_old_session = AgentSession::new(project_id, ProviderKind::Codex);
        renamed_old_session.created_at = 10;
        renamed_old_session.last_reply_at = Some(20);
        renamed_old_session.updated_at = 1_000;

        let mut newer_unanswered_session = AgentSession::new(project_id, ProviderKind::Codex);
        newer_unanswered_session.created_at = 30;
        newer_unanswered_session.last_reply_at = None;
        newer_unanswered_session.updated_at = 30;

        assert_eq!(sidebar_session_timestamp(&renamed_old_session), 20);
        assert_eq!(sidebar_session_timestamp(&newer_unanswered_session), 30);

        let mut sessions = vec![&renamed_old_session, &newer_unanswered_session];
        sort_sidebar_sessions(&mut sessions, SidebarOrdering::Updated);
        assert_eq!(sessions[0].id, newer_unanswered_session.id);

        sort_sidebar_sessions(&mut sessions, SidebarOrdering::Created);
        assert_eq!(sessions[0].id, newer_unanswered_session.id);
    }

    #[test]
    fn project_grouping_preserves_global_group_and_session_order() {
        let first_project = Uuid::from_u128(1);
        let second_project = Uuid::from_u128(2);
        let first = AgentSession::new(first_project, ProviderKind::Codex);
        let second = AgentSession::new(second_project, ProviderKind::Codex);
        let third = AgentSession::new(first_project, ProviderKind::Codex);

        let groups = project_sidebar_groups(&[&second, &first, &third], &HashSet::new());

        assert_eq!(
            groups,
            vec![
                (SidebarGroup::Project(second_project), vec![second.id]),
                (
                    SidebarGroup::Project(first_project),
                    vec![first.id, third.id]
                ),
            ]
        );
    }

    #[test]
    fn projectless_sessions_share_one_trailing_group() {
        let ordinary_project = Uuid::from_u128(1);
        let first_projectless_project = Uuid::from_u128(2);
        let second_projectless_project = Uuid::from_u128(3);
        let first_projectless = AgentSession::new(first_projectless_project, ProviderKind::Codex);
        let ordinary = AgentSession::new(ordinary_project, ProviderKind::Codex);
        let second_projectless = AgentSession::new(second_projectless_project, ProviderKind::Codex);

        let groups = project_sidebar_groups(
            &[&first_projectless, &ordinary, &second_projectless],
            &HashSet::from([first_projectless_project, second_projectless_project]),
        );

        assert_eq!(
            groups,
            vec![
                (SidebarGroup::Project(ordinary_project), vec![ordinary.id]),
                (
                    SidebarGroup::Projectless,
                    vec![first_projectless.id, second_projectless.id]
                ),
            ]
        );
    }

    #[test]
    fn projectless_sidebar_projects_are_paths_under_the_workspace_root() {
        let root = Path::new("/tmp/.insulator/projects");
        let projectless = Project {
            id: Uuid::from_u128(1),
            name: "Task".to_owned(),
            path: root.join("2026-08-23/task"),
            created_at: 0,
        };
        let ordinary = Project {
            id: Uuid::from_u128(2),
            name: "Ordinary".to_owned(),
            path: PathBuf::from("/tmp/dev/ordinary"),
            created_at: 0,
        };

        assert!(sidebar_project_is_projectless(&projectless, Some(root)));
        assert!(!sidebar_project_is_projectless(&ordinary, Some(root)));
        assert!(!sidebar_project_is_projectless(&projectless, None));
    }

    #[test]
    fn persisted_worktree_branches_supply_sidebar_labels() {
        let local = SessionWorkspace::Local;
        let planned = SessionWorkspace::NewWorktree {
            base_branch: Some("develop".to_owned()),
        };
        let worktree = SessionWorkspace::Worktree {
            path: PathBuf::from("/tmp/worktree"),
            branch: "feature/sidebar".to_owned(),
        };

        assert_eq!(persisted_sidebar_branch_label(&local), None);
        assert_eq!(persisted_sidebar_branch_label(&planned), Some("develop"));
        assert_eq!(
            persisted_sidebar_branch_label(&worktree),
            Some("feature/sidebar")
        );
    }

    #[test]
    fn pending_session_replaces_sidebar_selection_immediately() {
        let current = Uuid::from_u128(1);
        let pending = Uuid::from_u128(2);

        assert!(!sidebar_session_selected(
            Some(current),
            Some(pending),
            current
        ));
        assert!(sidebar_session_selected(
            Some(current),
            Some(pending),
            pending
        ));
        assert!(sidebar_session_selected(Some(current), None, current));
    }

    #[test]
    fn selected_session_uses_nearest_bottom_edge_for_an_unmeasured_lower_row() {
        let target = Uuid::from_u128(31);
        let group = SidebarGroup::Updated(SessionDateGroup::Today);
        let mut rows = vec![SidebarRow::Search, SidebarRow::Header(group)];
        rows.extend((1..=40).map(|id| SidebarRow::Session(Uuid::from_u128(id))));
        rows.push(SidebarRow::GroupSpacer);

        let index = sidebar_session_row_index(&rows, target).unwrap();
        let offset = sidebar_bottom_aligned_offset(&rows, index, px(400.0));

        assert_eq!(index, 32);
        assert_eq!(offset.item_ix, 21);
        assert_eq!(offset.offset_in_item, px(8.0));
        let visible_height = rows[offset.item_ix..=index]
            .iter()
            .copied()
            .map(sidebar_row_height)
            .fold(Pixels::ZERO, |height, row| height + row)
            - offset.offset_in_item;
        assert_eq!(visible_height, px(400.0));
        assert_eq!(sidebar_session_row_index(&rows, Uuid::from_u128(41)), None);
    }

    #[test]
    fn chat_status_order_and_properties() {
        assert_eq!(
            ChatStatus::MENU_ORDER,
            [
                ChatStatus::Backlog,
                ChatStatus::InProgress,
                ChatStatus::InReview,
                ChatStatus::Done,
                ChatStatus::Canceled,
            ]
        );
        assert_eq!(
            ChatStatus::GROUP_ORDER,
            [
                ChatStatus::Done,
                ChatStatus::InReview,
                ChatStatus::InProgress,
                ChatStatus::Backlog,
                ChatStatus::Canceled,
            ]
        );
        assert_eq!(ChatStatus::InProgress.label(), "In progress");
        assert_eq!(
            ChatStatus::InProgress.icon(),
            "icons/status-in-progress.svg"
        );
        assert_eq!(ChatStatus::Done.label(), "Done");
        assert_eq!(ChatStatus::Done.icon(), "icons/status-done.svg");
    }

    #[test]
    fn sidebar_status_grouping_rows() {
        let mut rows = Vec::new();
        let session_1 = Uuid::from_u128(1);
        let session_2 = Uuid::from_u128(2);

        rows.push(SidebarRow::WorkspacesHeader);
        for &status in &ChatStatus::GROUP_ORDER {
            let group = SidebarGroup::Status(status);
            let sessions = match status {
                ChatStatus::InProgress => vec![session_1],
                ChatStatus::Done => vec![session_2],
                _ => vec![],
            };
            append_sidebar_group_rows(&mut rows, group, &sessions, false, false);
        }

        assert_eq!(
            rows,
            vec![
                SidebarRow::WorkspacesHeader,
                SidebarRow::Header(SidebarGroup::Status(ChatStatus::Done)),
                SidebarRow::Session(session_2),
                SidebarRow::GroupSpacer,
                SidebarRow::Header(SidebarGroup::Status(ChatStatus::InReview)),
                SidebarRow::GroupSpacer,
                SidebarRow::Header(SidebarGroup::Status(ChatStatus::InProgress)),
                SidebarRow::Session(session_1),
                SidebarRow::GroupSpacer,
                SidebarRow::Header(SidebarGroup::Status(ChatStatus::Backlog)),
                SidebarRow::GroupSpacer,
                SidebarRow::Header(SidebarGroup::Status(ChatStatus::Canceled)),
                SidebarRow::GroupSpacer,
            ]
        );
    }

    #[test]
    fn dragged_session_payload_and_status_update() {
        let session_id = Uuid::from_u128(42);
        let dragged = DraggedSession {
            session_id,
            status: ChatStatus::Done,
            title: SharedString::from("Explaining Navier-Stokes"),
        };
        assert_eq!(dragged.session_id, session_id);
        assert_eq!(dragged.status, ChatStatus::Done);
        assert_eq!(dragged.title.as_ref(), "Explaining Navier-Stokes");

        // When dropped onto another status, target status differs
        let target_status = ChatStatus::InProgress;
        assert_ne!(dragged.status, target_status);
    }

    #[test]
    fn parses_various_github_remote_urls() {
        assert_eq!(
            parse_github_remote_url("git@github.com:owner/repo.git"),
            Some("https://github.com/owner/repo".to_string())
        );
        assert_eq!(
            parse_github_remote_url("git@github.com:owner/repo"),
            Some("https://github.com/owner/repo".to_string())
        );
        assert_eq!(
            parse_github_remote_url("https://github.com/owner/repo.git"),
            Some("https://github.com/owner/repo".to_string())
        );
        assert_eq!(
            parse_github_remote_url("https://github.com/owner/repo"),
            Some("https://github.com/owner/repo".to_string())
        );
        assert_eq!(
            parse_github_remote_url("ssh://git@github.com/owner/repo.git"),
            Some("https://github.com/owner/repo".to_string())
        );
        assert_eq!(
            parse_github_remote_url("git://github.com/owner/repo.git"),
            Some("https://github.com/owner/repo".to_string())
        );
        assert_eq!(
            parse_github_remote_url("git@gitlab.com:owner/repo.git"),
            None
        );
        assert_eq!(
            parse_github_remote_url("https://example.com/owner/repo.git"),
            None
        );
        assert_eq!(parse_github_remote_url(""), None);
    }

    #[test]
    fn detects_github_url_for_git_repo_or_none_for_non_git() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let url = github_url_for_project(&manifest_dir);
        assert!(url.is_some());
        assert!(url.unwrap().contains("github.com/"));

        let non_git = std::env::temp_dir();
        assert_eq!(github_url_for_project(&non_git), None);
    }
}
