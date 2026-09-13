//! The composer's autocompletion popup: slash commands and `@` file mentions.
//!
//! The popup is a pure view over prefetched data. Command and file indexes are
//! discovered on the background executor into `QueryCache`s and mirrored into
//! plain fields the frame reads; the filter over them is memoized per
//! keystroke, so a caret blink re-renders the popup without re-fuzzy-matching
//! the workspace.
//!
//! Keys follow the model picker's split: the composer keeps real focus the
//! whole time and the popup's selection is only drawn. While the popup is
//! open the composer card declares the `ComposerAutocomplete` key context, and
//! `up`/`down`/`enter`/`tab`/`escape` reach it as actions that outrank the
//! field's own bindings; when it closes the context disappears and `enter`
//! submits again.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gpui::{
    Anchor, App, Bounds, Font, KeyBinding, Pixels, StyledText, TextRun, anchored, deferred,
};
use nucleo_matcher::Matcher;

use crate::composer_complete::{
    self, FILE_INDEX_CAP, FileEntry, ReferenceEntry, Scored, SlashCommand, Trigger, TriggerKind,
    highlight_byte_ranges,
};
use crate::ui::menu::{ConfirmEntry, DismissMenu, SelectNextEntry, SelectPreviousEntry};

use super::composer::next_picker_highlight;
use super::*;

/// Key context the composer card declares while the popup is open.
const AUTOCOMPLETE_CONTEXT: &str = "ComposerAutocomplete > TextInput";
const AUTOCOMPLETE_LOADING_CONTEXT: &str = "ComposerAutocompleteLoading > TextInput";

/// Bind the popup's keys. Must run after [`crate::input::init`]: `enter` and
/// the arrows tie with the field's own bindings at the `ComposerInput` depth,
/// and the tie goes to whichever was registered last.
pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("down", SelectNextEntry, Some(AUTOCOMPLETE_CONTEXT)),
        KeyBinding::new("up", SelectPreviousEntry, Some(AUTOCOMPLETE_CONTEXT)),
        // The field binds the emacs spelling of the arrows too; while the
        // popup owns them they must move the highlight, not the caret.
        KeyBinding::new("ctrl-n", SelectNextEntry, Some(AUTOCOMPLETE_CONTEXT)),
        KeyBinding::new("ctrl-p", SelectPreviousEntry, Some(AUTOCOMPLETE_CONTEXT)),
        KeyBinding::new("enter", ConfirmEntry, Some(AUTOCOMPLETE_CONTEXT)),
        KeyBinding::new("tab", ConfirmEntry, Some(AUTOCOMPLETE_CONTEXT)),
        KeyBinding::new("escape", DismissMenu, Some(AUTOCOMPLETE_CONTEXT)),
        KeyBinding::new("escape", DismissMenu, Some(AUTOCOMPLETE_LOADING_CONTEXT)),
    ]);
}

pub(super) enum AutocompleteRow {
    Header(SharedString),
    Command(Scored<SlashCommand>),
    Reference(Scored<ReferenceEntry>),
    File(Scored<FileEntry>),
}

fn selectable_row_indexes(rows: &[AutocompleteRow]) -> Vec<usize> {
    rows.iter()
        .enumerate()
        .filter_map(|(index, row)| (!matches!(row, AutocompleteRow::Header(_))).then_some(index))
        .collect()
}

fn scroll_target_for_row_navigation(
    rows: &[AutocompleteRow],
    selectable: &[usize],
    next_pos: usize,
    key: &str,
) -> usize {
    let next = selectable.get(next_pos).copied().unwrap_or(0);
    if next_pos == 0 {
        // When landing on the first selectable item, always scroll to the very top (item 0)
        // so that any top header (such as "References") remains fully visible.
        0
    } else if key == "up"
        && next > 0
        && matches!(rows.get(next - 1), Some(AutocompleteRow::Header(_)))
    {
        // When navigating up into a section, scroll to the section's header so both
        // the header and the highlighted item stay in view.
        next - 1
    } else {
        next
    }
}

/// Filter results for one (kind, query, source index) — the popup's rows are
/// recomputed on a keystroke, not on every frame the caret blinks.
struct ResultsMemo {
    kind: TriggerKind,
    query: String,
    /// `Rc::as_ptr` identity of the source index the rows were filtered from.
    source: usize,
    rows: Rc<Vec<AutocompleteRow>>,
}

/// Cross-frame state for the popup. All interior-mutable: the render path
/// reconciles it from `&self`, the same way the transcript anchors do.
pub(super) struct AutocompleteUi {
    /// The trigger as of the last frame, for detecting query/site changes.
    token: RefCell<Option<Trigger>>,
    /// Keyboard cursor over the filtered rows: the popup opens with the first
    /// row selected, and every token change snaps back to it so the best
    /// match is always the one `enter` takes. Clamped to the list at use.
    highlight: Cell<usize>,
    /// Escape pressed on the current token; cleared the moment it changes.
    dismissed: Cell<bool>,
    scroll: ScrollHandle,
    /// The composer card's bounds as of the last frame, recorded by a probe,
    /// so the popup can anchor above the card at the card's own width.
    card_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    results: RefCell<Option<ResultsMemo>>,
    matcher: RefCell<Matcher>,
}

impl AutocompleteUi {
    pub(super) fn new() -> Self {
        Self {
            token: RefCell::new(None),
            highlight: Cell::new(0),
            dismissed: Cell::new(false),
            scroll: ScrollHandle::new(),
            card_bounds: Rc::new(Cell::new(None)),
            results: RefCell::new(None),
            matcher: RefCell::new(composer_complete::matcher()),
        }
    }

    /// The cell the composer card's bounds probe writes into.
    pub(super) fn card_bounds_cell(&self) -> Rc<Cell<Option<Bounds<Pixels>>>> {
        self.card_bounds.clone()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct PiReferenceFileFingerprint {
    modified: Option<std::time::SystemTime>,
    length: Option<u64>,
}

fn pi_reference_config_path(root: &std::path::Path, global: bool) -> std::path::PathBuf {
    let canonical = if global {
        dirs::home_dir()
            .map(|home| home.join(".pi/agent/references.json"))
            .unwrap_or_default()
    } else {
        root.join(".pi/references.json")
    };
    if canonical.is_file() {
        canonical
    } else if global {
        dirs::home_dir()
            .map(|home| home.join(".pi/agent/pi-refs.json"))
            .unwrap_or_default()
    } else {
        root.join(".pi/pi-refs.json")
    }
}

fn pi_reference_file_fingerprint(path: &std::path::Path) -> PiReferenceFileFingerprint {
    let metadata = std::fs::metadata(path).ok();
    PiReferenceFileFingerprint {
        modified: metadata
            .as_ref()
            .and_then(|metadata| metadata.modified().ok()),
        length: metadata.map(|metadata| metadata.len()),
    }
}

fn pi_reference_fingerprint(
    root: &std::path::Path,
) -> (PiReferenceFileFingerprint, PiReferenceFileFingerprint) {
    (
        pi_reference_file_fingerprint(&pi_reference_config_path(root, true)),
        pi_reference_file_fingerprint(&pi_reference_config_path(root, false)),
    )
}

impl Insulator {
    /// Keep Pi references current while the composer is open. The poll runs
    /// off the UI thread; rendering only sees the already-loaded cache.
    pub(super) fn start_pi_reference_watch(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |insulator, cx| {
            let mut previous: Option<(
                std::path::PathBuf,
                (PiReferenceFileFingerprint, PiReferenceFileFingerprint),
            )> = None;
            loop {
                let root = match insulator.update(cx, |insulator, _| {
                    insulator
                        .selected_session()
                        .filter(|session| session.provider == ProviderKind::Pi)
                        .and_then(|_| {
                            insulator
                                .selected_workspace_path()
                                .map(std::path::Path::to_path_buf)
                        })
                }) {
                    Ok(root) => root,
                    Err(_) => return,
                };
                let Some(root) = root else {
                    previous = None;
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(500))
                        .await;
                    continue;
                };
                let (root, fingerprint) = cx
                    .background_executor()
                    .spawn(async move {
                        let fingerprint = pi_reference_fingerprint(&root);
                        (root, fingerprint)
                    })
                    .await;
                let changed =
                    previous
                        .as_ref()
                        .is_some_and(|(previous_root, previous_fingerprint)| {
                            previous_root == &root && previous_fingerprint != &fingerprint
                        });
                previous = Some((root, fingerprint));
                if changed {
                    let _ = insulator.update(cx, |insulator, cx| {
                        insulator.invalidate_composer_sources(cx);
                        cx.notify();
                    });
                }
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(500))
                    .await;
            }
        })
        .detach();
    }

    /// Refresh the drawn command and file indexes for the selected session.
    ///
    /// A cache hit lands immediately; a miss starts discovery on the
    /// background executor and re-runs this when it arrives. Nothing here may
    /// touch the filesystem directly.
    pub(super) fn refresh_composer_sources(&mut self, cx: &mut Context<Self>) {
        let Some(project_path) = self
            .selected_workspace_path()
            .map(std::path::Path::to_path_buf)
        else {
            self.slash_command_index = Rc::new(Vec::new());
            self.slash_command_index_key = None;
            self.slash_command_index_loading = false;
            self.mention_file_index = Rc::new(Vec::new());
            self.mention_reference_index = Rc::new(Vec::new());
            self.mention_file_index_path = None;
            self.mention_file_index_loading = false;
            return;
        };
        let provider = self
            .selected_session()
            .map(|session| session.provider)
            .unwrap_or(self.state.last_provider);
        let reported = self
            .selected_session()
            .map(|session| session.available_commands.clone())
            .unwrap_or_default();
        let binary_override = self.state.provider_binary_overrides.get(&provider).cloned();

        let command_key = (provider, project_path.clone(), binary_override.clone());
        match self.slash_commands.read(&command_key) {
            Query::Ready(commands) => {
                self.slash_command_index = Rc::new(composer_complete::merge_reported_commands(
                    &commands, &reported,
                ));
                self.slash_command_index_key = Some(command_key);
                self.slash_command_index_loading = false;
            }
            Query::Pending => {
                self.slash_command_index_loading = true;
                // A scan for this exact key is in flight; anything drawn
                // meanwhile must not be another provider's list.
                if self.slash_command_index_key.as_ref() != Some(&command_key) {
                    self.slash_command_index = Rc::new(Vec::new());
                    self.slash_command_index_key = None;
                }
            }
            Query::Missing(token) => {
                self.slash_command_index_loading = true;
                if self.slash_command_index_key.as_ref() != Some(&command_key) {
                    self.slash_command_index = Rc::new(Vec::new());
                    self.slash_command_index_key = None;
                }
                let path = project_path.clone();
                let workspace = insulator_client::WorkspaceClient::new(self.daemon.client());
                cx.spawn(async move |insulator, cx| {
                    let commands = cx
                        .background_executor()
                        .spawn(async move {
                            match workspace.request(
                                insulator_client::WorkspaceOperation::DiscoverSlashCommands {
                                    provider,
                                    project_root: path,
                                    binary_override,
                                },
                            ) {
                                Ok(insulator_client::WorkspaceResult::SlashCommands {
                                    commands,
                                }) => commands,
                                Ok(_) | Err(_) => Vec::new(),
                            }
                        })
                        .await;
                    insulator
                        .update(cx, |insulator, cx| {
                            if insulator.slash_commands.fulfill(token, commands) {
                                insulator.refresh_composer_sources(cx);
                                cx.notify();
                            }
                        })
                        .ok();
                })
                .detach();
            }
        }

        match self.mention_files.read(&project_path) {
            Query::Ready(value) => {
                self.mention_file_index = value.0.clone().into();
                self.mention_reference_index = if provider == ProviderKind::Pi {
                    value.1.clone().into()
                } else {
                    Rc::new(Vec::new())
                };
                self.mention_file_index_path = Some(project_path);
                self.mention_file_index_loading = false;
            }
            Query::Pending => {
                self.mention_file_index_loading = true;
                if self.mention_file_index_path.as_ref() != Some(&project_path) {
                    self.mention_file_index = Rc::new(Vec::new());
                    self.mention_reference_index = Rc::new(Vec::new());
                    self.mention_file_index_path = None;
                }
            }
            Query::Missing(token) => {
                self.mention_file_index_loading = true;
                if self.mention_file_index_path.as_ref() != Some(&project_path) {
                    self.mention_file_index = Rc::new(Vec::new());
                    self.mention_reference_index = Rc::new(Vec::new());
                    self.mention_file_index_path = None;
                }
                let path = project_path.clone();
                let workspace = insulator_client::WorkspaceClient::new(self.daemon.client());
                cx.spawn(async move |insulator, cx| {
                    let files = cx
                        .background_executor()
                        .spawn(async move {
                            match workspace.request(
                                insulator_client::WorkspaceOperation::ListProjectFiles {
                                    root: path,
                                    cap: FILE_INDEX_CAP,
                                },
                            ) {
                                Ok(insulator_client::WorkspaceResult::ProjectFiles {
                                    entries,
                                    references,
                                }) => (entries, references),
                                Ok(_) | Err(_) => (Vec::new(), Vec::new()),
                            }
                        })
                        .await;
                    insulator
                        .update(cx, |insulator, cx| {
                            if insulator.mention_files.fulfill(token, files) {
                                insulator.refresh_composer_sources(cx);
                                cx.notify();
                            }
                        })
                        .ok();
                })
                .detach();
            }
        }
    }

    /// Invalidate and re-request both indexes for the selected workspace.
    pub(super) fn invalidate_composer_sources(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self
            .selected_workspace_path()
            .map(std::path::Path::to_path_buf)
        {
            let provider = self
                .selected_session()
                .map(|session| session.provider)
                .unwrap_or(self.state.last_provider);
            let binary_override = self.state.provider_binary_overrides.get(&provider).cloned();
            self.slash_commands
                .invalidate(&(provider, path.clone(), binary_override));
            self.mention_files.invalidate(&path);
        }
        self.refresh_composer_sources(cx);
    }

    /// The trigger under the composer's caret, reconciled with the popup's
    /// cross-frame state. `None` while the composer is unfocused, the token is
    /// dismissed, or there is nothing to complete.
    fn composer_trigger(&self, window: &Window, cx: &App) -> Option<Trigger> {
        let input = self.composer.read(cx);
        let trigger = if input.focus().is_focused(window) {
            composer_complete::detect_trigger(input.content(cx), input.cursor(cx))
        } else {
            None
        };
        let ui = &self.composer_autocomplete;
        if *ui.token.borrow() != trigger {
            *ui.token.borrow_mut() = trigger.clone();
            // A different token renumbers the rows: the keyboard cursor and a
            // standing dismissal both describe the previous list.
            ui.highlight.set(0);
            ui.dismissed.set(false);
            ui.scroll.scroll_to_item(0);
        }
        if ui.dismissed.get() {
            return None;
        }
        trigger
    }

    /// The filtered rows for `trigger`, shared by the popup body, the keyboard
    /// cursor and `enter` so an index always means the same row everywhere.
    fn autocomplete_rows(&self, trigger: &Trigger) -> Rc<Vec<AutocompleteRow>> {
        let source = match trigger.kind {
            TriggerKind::Command => Rc::as_ptr(&self.slash_command_index) as usize,
            TriggerKind::File => {
                (Rc::as_ptr(&self.mention_file_index) as usize)
                    ^ (Rc::as_ptr(&self.mention_reference_index) as usize).rotate_left(1)
            }
        };
        {
            let memo = self.composer_autocomplete.results.borrow();
            if let Some(memo) = memo.as_ref().filter(|memo| {
                memo.kind == trigger.kind && memo.query == trigger.query && memo.source == source
            }) {
                return memo.rows.clone();
            }
        }
        let mut matcher = self.composer_autocomplete.matcher.borrow_mut();
        let rows = match trigger.kind {
            TriggerKind::Command => composer_complete::filter_commands(
                &self.slash_command_index,
                &trigger.query,
                &mut matcher,
            )
            .into_iter()
            .map(AutocompleteRow::Command)
            .collect::<Vec<_>>(),
            TriggerKind::File => {
                let references = composer_complete::filter_references(
                    &self.mention_reference_index,
                    &trigger.query,
                    &mut matcher,
                );
                let files = composer_complete::filter_files(
                    &self.mention_file_index,
                    &trigger.query,
                    &mut matcher,
                );
                let mut rows = Vec::with_capacity(references.len() + files.len() + 2);
                if !references.is_empty() {
                    rows.push(AutocompleteRow::Header("References".into()));
                    rows.extend(references.into_iter().map(AutocompleteRow::Reference));
                }
                if !files.is_empty() {
                    rows.push(AutocompleteRow::Header("Files".into()));
                    rows.extend(files.into_iter().map(AutocompleteRow::File));
                }
                rows
            }
        };
        let rows = Rc::new(rows);
        *self.composer_autocomplete.results.borrow_mut() = Some(ResultsMemo {
            kind: trigger.kind,
            query: trigger.query.clone(),
            source,
            rows: rows.clone(),
        });
        rows
    }

    pub(super) fn move_autocomplete_highlight(
        &mut self,
        key: &str,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(trigger) = self.composer_trigger(window, cx) else {
            return;
        };
        let rows = self.autocomplete_rows(&trigger);
        let selectable = selectable_row_indexes(&rows);
        if selectable.is_empty() {
            return;
        }
        let ui = &self.composer_autocomplete;
        let current = selectable
            .iter()
            .position(|index| *index == ui.highlight.get())
            .unwrap_or(0);
        let Some(next_pos) = next_picker_highlight(Some(current), selectable.len(), key) else {
            return;
        };
        let next = selectable[next_pos];
        ui.highlight.set(next);
        let scroll_target = scroll_target_for_row_navigation(&rows, &selectable, next_pos, key);
        ui.scroll.scroll_to_item(scroll_target);
        cx.notify();
    }

    /// Insert the chosen row over the trigger token. `index` comes from a
    /// click; `None` is the keyboard path, which takes the drawn cursor and
    /// defaults to the first row so `enter` works the moment the popup opens.
    pub(super) fn accept_autocomplete(
        &mut self,
        index: Option<usize>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(trigger) = self.composer_trigger(window, cx) else {
            return;
        };
        let rows = self.autocomplete_rows(&trigger);
        let index = index.unwrap_or_else(|| {
            let highlighted = self.composer_autocomplete.highlight.get();
            selectable_row_indexes(&rows)
                .into_iter()
                .find(|index| *index == highlighted)
                .or_else(|| selectable_row_indexes(&rows).into_iter().next())
                .unwrap_or(0)
        });
        let Some(row) = rows.get(index) else {
            return;
        };
        let insert = match row {
            AutocompleteRow::Command(scored) => {
                let composer_text = composer_complete::command_composer_text(&scored.item);
                format!("{composer_text} ")
            }
            AutocompleteRow::Reference(scored) => format!("@{} ", scored.item.alias),
            AutocompleteRow::File(scored) => format!("@{} ", scored.item.path),
            AutocompleteRow::Header(_) => return,
        };
        if matches!(row, AutocompleteRow::Command(_)) {
            let mut submission = self.composer.read(cx).content(cx).to_owned();
            submission.replace_range(trigger.range.clone(), &insert);
            if self.execute_local_composer_command(&submission, cx) {
                return;
            }
        }
        self.composer.update(cx, |input, cx| {
            input.replace_range(trigger.range.clone(), &insert, cx);
        });
        cx.notify();
    }

    pub(super) fn dismiss_autocomplete(&mut self, cx: &mut Context<Self>) {
        self.composer_autocomplete.dismissed.set(true);
        cx.notify();
    }

    /// The popup, anchored above the composer card, or `None` when idle.
    ///
    /// Reads only the prefetched indexes — discovery never runs on a frame.
    pub(super) fn render_composer_autocomplete(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<(AnyElement, bool)> {
        let trigger = self.composer_trigger(window, cx)?;
        let rows = self.autocomplete_rows(&trigger);
        let loading = match trigger.kind {
            TriggerKind::Command => self.slash_command_index_loading,
            TriggerKind::File => self.mention_file_index_loading,
        };
        if rows.is_empty() && !loading {
            return None;
        }
        // The probe records during paint, so the first frame a composer ever
        // draws has no bounds yet; the popup appears one frame later.
        let card_bounds = self.composer_autocomplete.card_bounds.get()?;
        let theme = Theme::current(cx);
        let selectable = selectable_row_indexes(&rows);
        let highlight = if selectable.contains(&self.composer_autocomplete.highlight.get()) {
            self.composer_autocomplete.highlight.get()
        } else {
            let first = selectable.first().copied().unwrap_or(0);
            self.composer_autocomplete.highlight.set(first);
            first
        };

        let mut list = div()
            .id("composer-autocomplete-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.composer_autocomplete.scroll)
            .p(px(4.0));
        if rows.is_empty() {
            list = list.child(
                div()
                    .h(px(30.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(sp(12.5))
                    .text_color(theme.text_tertiary)
                    .child(crate::app::components::dot_matrix_loader(
                        theme.text_tertiary,
                        12.0,
                    ))
                    .child(tr!("composer.loading_suggestions")),
            );
        } else {
            for (index, row) in rows.iter().enumerate() {
                list = list
                    .child(self.render_autocomplete_row(index, row, highlight, &theme, window, cx));
            }
        }

        Some((
            deferred(
                anchored()
                    .position(point(card_bounds.origin.x, card_bounds.origin.y - px(6.0)))
                    .anchor(Anchor::BottomLeft)
                    .snap_to_window_with_margin(px(8.0))
                    .child(
                        div()
                            .occlude()
                            .w(card_bounds.size.width)
                            .max_h(px(302.0))
                            .rounded(px(11.0))
                            .font_family(crate::theme::active_ui_font_family())
                            .border_1()
                            .border_color(theme.border_strong)
                            .bg(theme.elevated)
                            .shadow_lg()
                            .flex()
                            .flex_col()
                            .overflow_hidden()
                            .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                                this.dismiss_autocomplete(cx);
                            }))
                            .child(list),
                    ),
            )
            .with_priority(1)
            .into_any_element(),
            !rows.is_empty(),
        ))
    }

    fn render_autocomplete_row(
        &self,
        index: usize,
        row: &AutocompleteRow,
        highlight: usize,
        theme: &Theme,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let highlighted = highlight == index;
        let mut font = window.text_style().font();
        font.family = SharedString::from(crate::theme::active_ui_font_family());
        if let AutocompleteRow::Header(label) = row {
            return div()
                .id(index)
                .h(px(24.0))
                .px(px(8.0))
                .flex()
                .items_center()
                .text_size(sp(11.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(theme.text_ghost)
                .child(label.clone())
                .into_any_element();
        }
        let base = div()
            .id(index)
            .h(px(30.0))
            .px(px(8.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_default()
            .when(highlighted, |element| element.bg(theme.overlay_strong))
            .hover(|element| element.bg(theme.overlay))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    this.accept_autocomplete(Some(index), window, cx);
                }),
            );
        match row {
            AutocompleteRow::Header(_) => unreachable!("headers return before selectable rows"),
            AutocompleteRow::Command(scored) => {
                let command = &scored.item;
                let composer_text = composer_complete::command_composer_text(command);
                let icon_path = if command.scope == composer_complete::CommandScope::Skill {
                    "icons/sparkle.svg"
                } else {
                    "icons/command.svg"
                };
                // Positions index the bare name; the drawn sigil shifts every
                // byte range right by one.
                let name_ranges = highlight_byte_ranges(&command.name, &scored.positions, 0)
                    .into_iter()
                    .map(|range| range.start + 1..range.end + 1)
                    .collect();
                let mut name_font = font.clone();
                name_font.weight = FontWeight::MEDIUM;
                base.child(icon(icon_path, 12.0, theme.text_tertiary))
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(260.0))
                            .truncate()
                            .text_size(sp(12.5))
                            .child(matched_text(
                                composer_text,
                                name_ranges,
                                theme.text,
                                theme.accent,
                                name_font,
                            )),
                    )
                    .when_some(command.argument_hint.clone(), |element, hint| {
                        element.child(
                            div()
                                .flex_none()
                                .text_size(sp(12.5))
                                .text_color(theme.text_ghost)
                                .child(hint),
                        )
                    })
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(sp(12.5))
                            .text_color(theme.text_tertiary)
                            .child(SharedString::from(command.description.clone())),
                    )
                    .child(
                        div()
                            .h(px(18.0))
                            .px(px(5.0))
                            .flex_none()
                            .rounded(px(4.0))
                            .border_1()
                            .border_color(theme.border)
                            .flex()
                            .items_center()
                            .text_size(sp(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text_tertiary)
                            .child(command.scope.label()),
                    )
                    .into_any_element()
            }
            AutocompleteRow::Reference(scored) => {
                let reference = &scored.item;
                base.child(icon("icons/git-branch.svg", 13.0, theme.text_tertiary))
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(260.0))
                            .truncate()
                            .text_size(sp(12.5))
                            .child(matched_text(
                                format!("@{}", reference.alias),
                                highlight_byte_ranges(&reference.alias, &scored.positions, 0)
                                    .into_iter()
                                    .map(|range| range.start + 1..range.end + 1)
                                    .collect(),
                                theme.text,
                                theme.accent,
                                font,
                            )),
                    )
                    .when_some(reference.description.clone(), |element, description| {
                        element.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(sp(12.5))
                                .text_color(theme.text_tertiary)
                                .child(description),
                        )
                    })
                    .into_any_element()
            }
            AutocompleteRow::File(scored) => {
                let file = &scored.item;
                // The row draws basename then directory, but the positions
                // index the full path — each segment recovers its own ranges.
                // A directory's trailing slash stays with the basename, so a
                // match on it still paints.
                let trimmed_len = file.path.trim_end_matches('/').len();
                let name_start = file.path[..trimmed_len]
                    .rfind('/')
                    .map_or(0, |index| index + 1);
                let name = &file.path[name_start..];
                let parent = &file.path[..name_start.saturating_sub(1)];
                let name_char_offset = file.path[..name_start].chars().count();
                let icon_path = if file.is_dir {
                    "icons/folder.svg"
                } else {
                    super::right_panel::file_icon_for_path(&file.path)
                };
                base.child(icon(icon_path, 13.0, theme.text_tertiary))
                    .child(
                        div()
                            .flex_none()
                            .max_w(px(300.0))
                            .truncate()
                            .text_size(sp(12.5))
                            .child(matched_text(
                                name.to_owned(),
                                highlight_byte_ranges(name, &scored.positions, name_char_offset),
                                theme.text,
                                theme.accent,
                                font.clone(),
                            )),
                    )
                    .when(!parent.is_empty(), |element| {
                        element.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_size(sp(12.5))
                                .child(matched_text(
                                    parent.to_owned(),
                                    highlight_byte_ranges(parent, &scored.positions, 0),
                                    theme.text_ghost,
                                    theme.accent,
                                    font,
                                )),
                        )
                    })
                    .into_any_element()
            }
        }
    }
}

/// Text with the fuzzy-matched byte ranges lifted to the accent colour and a
/// semibold weight, the runs tiling the string exactly. `ranges` are sorted
/// and non-overlapping, as [`highlight_byte_ranges`] returns them.
fn matched_text(
    text: String,
    ranges: Vec<std::ops::Range<usize>>,
    base_color: Hsla,
    accent: Hsla,
    font: Font,
) -> StyledText {
    // A step above either base weight in these rows — regular file paths and
    // medium command names both read as "a bit bolder", not shouting.
    let mut accent_font = font.clone();
    accent_font.weight = FontWeight::SEMIBOLD;
    let run = |len: usize, font: Font, color: Hsla| TextRun {
        len,
        font,
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let mut runs = Vec::new();
    let mut cursor = 0;
    for range in ranges {
        if range.start > cursor {
            runs.push(run(range.start - cursor, font.clone(), base_color));
        }
        runs.push(run(range.len(), accent_font.clone(), accent));
        cursor = range.end;
    }
    if cursor < text.len() {
        runs.push(run(text.len() - cursor, font.clone(), base_color));
    }
    StyledText::new(text).with_runs(runs)
}

/// The probe recording the composer card's bounds for the popup's anchor.
/// `inset_0` so it reports the border box, not the padded content box.
pub(super) fn composer_card_bounds_probe(
    cell: Rc<Cell<Option<Bounds<Pixels>>>>,
) -> impl IntoElement {
    canvas(
        move |bounds: Bounds<Pixels>, _, _| cell.set(Some(bounds)),
        |_, _, _, _| (),
    )
    .absolute()
    .inset_0()
}

#[cfg(test)]
mod tests {
    /// The popup renders on every keystroke frame; discovery walks the
    /// filesystem and forks subprocesses. The two must never meet: everything
    /// the render path shows comes from the prefetched indexes.
    #[test]
    fn the_autocomplete_render_path_does_no_filesystem_work() {
        let source = include_str!("./autocomplete.rs");
        let start = source
            .find("\n    fn composer_trigger(")
            .expect("composer_trigger must exist");
        let end = source
            .find("\n/// The probe recording")
            .expect("probe marker must exist");
        let render_paths = &source[start..end];
        for forbidden in [
            "discover_slash_commands(",
            "list_project_files(",
            "std::fs",
            "Command::new",
            "read_dir",
        ] {
            assert!(
                !render_paths.contains(forbidden),
                "the render path must not call `{forbidden}`; \
                 discovery belongs in refresh_composer_sources"
            );
        }
    }

    #[test]
    fn scroll_target_keeps_headers_visible() {
        use super::*;

        let rows = vec![
            AutocompleteRow::Header("References".into()),
            AutocompleteRow::Reference(Scored {
                positions: vec![],
                item: ReferenceEntry {
                    alias: "effect".into(),
                    description: Some("Effect-TS".into()),
                },
            }),
            AutocompleteRow::Reference(Scored {
                positions: vec![],
                item: ReferenceEntry {
                    alias: "opencode".into(),
                    description: Some("tui".into()),
                },
            }),
            AutocompleteRow::Header("Files".into()),
            AutocompleteRow::File(Scored {
                positions: vec![],
                item: FileEntry {
                    path: ".gitignore".into(),
                    is_dir: false,
                },
            }),
        ];
        let selectable = selectable_row_indexes(&rows);
        assert_eq!(selectable, vec![1, 2, 4]);

        // Navigating to first item (pos 0) always scrolls to 0 (top header)
        assert_eq!(
            scroll_target_for_row_navigation(&rows, &selectable, 0, "up"),
            0
        );
        assert_eq!(
            scroll_target_for_row_navigation(&rows, &selectable, 0, "down"),
            0
        );

        // Navigating down to pos 1 (opencode) scrolls to 2
        assert_eq!(
            scroll_target_for_row_navigation(&rows, &selectable, 1, "down"),
            2
        );

        // Navigating down to pos 2 (.gitignore) scrolls to 4
        assert_eq!(
            scroll_target_for_row_navigation(&rows, &selectable, 2, "down"),
            4
        );

        // Navigating up to pos 2 (.gitignore, preceded by "Files" header at index 3) scrolls to 3
        assert_eq!(
            scroll_target_for_row_navigation(&rows, &selectable, 2, "up"),
            3
        );

        // Navigating up to pos 1 (opencode, preceded by "effect" at index 1) scrolls to 2
        assert_eq!(
            scroll_target_for_row_navigation(&rows, &selectable, 1, "up"),
            2
        );

        // Navigating up to pos 0 (effect, preceded by "References" header at index 0) scrolls to 0
        assert_eq!(
            scroll_target_for_row_navigation(&rows, &selectable, 0, "up"),
            0
        );
    }
}
