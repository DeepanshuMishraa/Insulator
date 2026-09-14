use super::*;

use anyhow::Context as _;
use base64::Engine as _;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ComposerSubmitAction {
    Send,
    Preparing,
    Stop,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) enum PlanMode {
    #[default]
    Off,
    Plan,
    Plannotator,
}

impl PlanMode {
    fn label(self) -> &'static str {
        match self {
            Self::Off => "No plan",
            Self::Plan => "Plan mode",
            Self::Plannotator => "Plannotator plan mode",
        }
    }
}

/// Providers with a native read-only planning state and how the toggle
/// reaches it. Pi and Oh My Pi share prompt-text toggles (`/plan`,
/// `/plannotator-plan-mode`); OpenCode and OpenCode 2 switch their native
/// `plan` agent; Cursor flips its ACP session mode and Fx flips its own
/// (`ask` is its restrictive mode, `code` its ordinary one); Claude switches
/// its `plan` permission mode live via `set_permission_mode`; Codex pins its
/// per-turn sandbox to `read-only`; DeepSeek Harness runs its `/plan`
/// command. Amp, Grok, and Kimi have no live-switchable plan state here: Amp
/// exposes no permission surface at all, and Grok and Kimi advertise no plan
/// session mode, so they stay out of the toggle until one exists.
pub(super) fn plan_mode_supported(provider: ProviderKind) -> bool {
    matches!(
        provider,
        ProviderKind::Pi
            | ProviderKind::OhMyPi
            | ProviderKind::OpenCode
            | ProviderKind::OpenCode2
            | ProviderKind::Cursor
            | ProviderKind::Fx
            | ProviderKind::Claude
            | ProviderKind::Codex
            | ProviderKind::DeepSeek
    )
}

pub(super) fn plan_mode_commands(
    provider: ProviderKind,
    current: PlanMode,
    target: PlanMode,
) -> Vec<String> {
    if current == target {
        return Vec::new();
    }
    match provider {
        ProviderKind::OpenCode | ProviderKind::OpenCode2 => {
            let agent = match target {
                PlanMode::Off => "build",
                PlanMode::Plan | PlanMode::Plannotator => "plan",
            };
            return vec![format!("agent:{agent}")];
        }
        ProviderKind::Cursor => {
            let mode = match target {
                PlanMode::Off => "agent",
                PlanMode::Plan | PlanMode::Plannotator => "plan",
            };
            return vec![format!("mode:{mode}")];
        }
        ProviderKind::Fx => {
            let mode = match target {
                PlanMode::Off => "code",
                PlanMode::Plan | PlanMode::Plannotator => "ask",
            };
            return vec![format!("mode:{mode}")];
        }
        ProviderKind::Claude => {
            return vec![match target {
                PlanMode::Off => "permission:restore".to_owned(),
                PlanMode::Plan | PlanMode::Plannotator => "permission:plan".to_owned(),
            }];
        }
        ProviderKind::Codex => {
            return vec![match target {
                PlanMode::Off => "sandbox:restore".to_owned(),
                PlanMode::Plan | PlanMode::Plannotator => "sandbox:read-only".to_owned(),
            }];
        }
        ProviderKind::DeepSeek => {
            return vec![match target {
                PlanMode::Off => "/plan off".to_owned(),
                PlanMode::Plan | PlanMode::Plannotator => "/plan".to_owned(),
            }];
        }
        ProviderKind::Pi | ProviderKind::OhMyPi => {}
        _ => return Vec::new(),
    }
    let mut commands = Vec::with_capacity(2);
    match current {
        PlanMode::Off => {}
        PlanMode::Plan => commands.push("/plan".to_owned()),
        PlanMode::Plannotator => commands.push("/plannotator-plan-mode".to_owned()),
    }
    match target {
        PlanMode::Off => {}
        PlanMode::Plan => commands.push("/plan".to_owned()),
        PlanMode::Plannotator => commands.push("/plannotator-plan-mode".to_owned()),
    }
    commands
}

/// Backwards-compatible alias: Pi was the first provider behind the toggle.
#[allow(dead_code)]
pub(super) type PiPlanMode = PlanMode;

#[allow(dead_code)]
pub(super) fn pi_plan_mode_commands(current: PiPlanMode, target: PiPlanMode) -> Vec<String> {
    plan_mode_commands(ProviderKind::Pi, current, target)
}

pub(super) fn composer_submit_action(
    status: Option<SessionStatus>,
    preparing: bool,
) -> ComposerSubmitAction {
    if preparing {
        ComposerSubmitAction::Preparing
    } else if status.is_some_and(SessionStatus::is_busy) {
        ComposerSubmitAction::Stop
    } else {
        ComposerSubmitAction::Send
    }
}

impl Insulator {
    // ── Permission ─────────────────────────────────────────────────────────

    pub(super) fn render_permission(&self, cx: &mut Context<Self>) -> Option<Div> {
        if let Some(input) = self.selected_runtime()?.pending_user_input.clone() {
            return Some(self.render_user_input(input, cx));
        }
        if let Some(permission) = self.selected_runtime()?.pending_computer_approval.as_ref() {
            return Some(self.render_computer_permission(permission, cx));
        }
        let permission = self.selected_runtime()?.pending_permission.as_ref()?;
        let theme = Theme::current(cx);
        let request_id = permission.request_id.clone();
        let mut buttons = div().flex().items_center().gap(px(8.0)).mt(px(10.0));
        for option in &permission.options {
            let request_id = request_id.clone();
            let option_id = option.id.clone();
            let allow = option.allow;
            buttons = buttons.child(
                div()
                    .id(SharedString::from(format!(
                        "permission-{}-{}",
                        permission.request_id, option.id
                    )))
                    .h(px(28.0))
                    .px(px(13.0))
                    .rounded(px(7.0))
                    .flex()
                    .items_center()
                    .cursor_default()
                    .text_size(sp(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .when(allow, |element| {
                        element
                            .bg(theme.inverse)
                            .text_color(theme.on_inverse)
                            .hover(|element| element.opacity(0.9))
                    })
                    .when(!allow, |element| {
                        element
                            .border_1()
                            .border_color(theme.border_strong)
                            .text_color(theme.text_secondary)
                            .hover(|element| element.bg(theme.overlay).text_color(theme.text))
                    })
                    .active(|element| element.opacity(0.8))
                    .child(SharedString::from(option.label.clone()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.respond_permission(request_id.clone(), option_id.clone(), cx);
                    })),
            );
        }
        Some(
            div().px(px(20.0)).pb(px(8.0)).child(
                div()
                    .w_full()
                    .max_w(px(CONTENT_MAX_WIDTH))
                    .mx_auto()
                    .p(px(12.0))
                    .rounded(px(12.0))
                    .border_1()
                    .border_color(theme.border_strong)
                    .bg(theme.raised)
                    .shadow_md()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(icon("icons/alert.svg", 13.0, theme.warning))
                            .child(
                                div()
                                    .text_size(sp(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child(SharedString::from(permission.title.clone())),
                            ),
                    )
                    .child(
                        div()
                            .id("permission-detail")
                            .mt(px(8.0))
                            .max_h(px(92.0))
                            .overflow_y_scroll()
                            .p(px(8.0))
                            .rounded(px(7.0))
                            .bg(theme.inset)
                            .font_family(crate::md::render::active_mono_family())
                            .text_size(sp(12.5))
                            .line_height(sp(16.0))
                            .text_color(theme.text_secondary)
                            .whitespace_normal()
                            .child(SharedString::from(permission.detail.clone())),
                    )
                    .child(buttons),
            ),
        )
    }

    fn render_user_input(&self, pending: PendingUserInput, cx: &mut Context<Self>) -> Div {
        let theme = Theme::current(cx);
        let Some(question) = pending.current_question().cloned() else {
            return div();
        };
        let selected = pending
            .selections
            .get(&question.id)
            .cloned()
            .unwrap_or_default();
        let has_custom = pending
            .custom_answers
            .get(&question.id)
            .is_some_and(|answer| !answer.trim().is_empty());
        let can_continue = has_custom || !selected.is_empty();
        let is_last = pending.question_index + 1 == pending.questions.len();
        let request_id = pending.request_id.clone();
        let question_index = pending.question_index;
        let mut options = div().mt(px(9.0)).flex().flex_col().gap(px(4.0));
        for (index, option) in question.options.iter().enumerate() {
            let is_selected = selected.iter().any(|answer| answer == &option.label);
            let click_label = option.label.clone();
            let key_label = option.label.clone();
            let focus = self.transcript_control_focus(
                format!("user-input-{request_id}-{question_index}-option-{index}"),
                cx,
            );
            options = options.child(
                div()
                    .id(SharedString::from(format!(
                        "user-input-{request_id}-{question_index}-option-{index}"
                    )))
                    .track_focus(&focus)
                    .tab_index(0)
                    .tab_stop(true)
                    .min_h(px(36.0))
                    .px(px(10.0))
                    .py(px(5.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(if is_selected {
                        theme.accent.opacity(0.34)
                    } else {
                        theme.border.opacity(0.0)
                    })
                    .bg(if is_selected {
                        theme.accent.opacity(0.08)
                    } else {
                        theme.overlay
                    })
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .cursor_default()
                    .focus_visible(|style| style.border_color(theme.accent))
                    .when(!is_selected, |row| {
                        row.hover(|style| style.border_color(theme.border).bg(theme.overlay_strong))
                    })
                    .active(|style| style.opacity(0.85))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(sp(12.5))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child(SharedString::from(option.label.clone())),
                            )
                            .children(option.description.as_ref().map(|description| {
                                div()
                                    .mt(px(1.0))
                                    .text_size(sp(12.5))
                                    .line_height(sp(15.0))
                                    .text_color(theme.text_secondary)
                                    .whitespace_normal()
                                    .child(SharedString::from(description.clone()))
                            })),
                    )
                    .when(is_selected, |row| {
                        row.child(icon("icons/check.svg", 12.0, theme.accent))
                    })
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.select_user_input_option(click_label.clone(), cx);
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.select_user_input_option(key_label.clone(), cx);
                            cx.stop_propagation();
                        }
                    })),
            );
        }

        let next_focus = self.transcript_control_focus(
            format!("user-input-{request_id}-{question_index}-continue"),
            cx,
        );
        let back = (question_index > 0).then(|| {
            let focus = self.transcript_control_focus(
                format!("user-input-{request_id}-{question_index}-back"),
                cx,
            );
            div()
                .id(SharedString::from(format!(
                    "user-input-{request_id}-{question_index}-back"
                )))
                .track_focus(&focus)
                .tab_index(0)
                .tab_stop(true)
                .h(px(26.0))
                .px(px(8.0))
                .rounded(px(6.0))
                .flex()
                .items_center()
                .cursor_default()
                .text_size(sp(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_tertiary)
                .focus_visible(|style| style.border_1().border_color(theme.accent))
                .hover(|style| style.bg(theme.overlay).text_color(theme.text_secondary))
                .active(|style| style.opacity(0.8))
                .child(tr!("user_input.back"))
                .on_click(cx.listener(|this, _, _, cx| this.previous_user_input(cx)))
                .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        this.previous_user_input(cx);
                        cx.stop_propagation();
                    }
                }))
        });
        let continue_button = div()
            .id(SharedString::from(format!(
                "user-input-{request_id}-{question_index}-continue"
            )))
            .track_focus(&next_focus)
            .tab_index(0)
            .tab_stop(can_continue)
            .h(px(26.0))
            .px(px(10.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .cursor_default()
            .text_size(sp(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .bg(if can_continue {
                theme.inverse
            } else {
                theme.overlay
            })
            .text_color(if can_continue {
                theme.on_inverse
            } else {
                theme.text_ghost
            })
            .when(can_continue, |button| {
                button
                    .focus_visible(|style| style.border_1().border_color(theme.accent))
                    .hover(|style| style.opacity(0.9))
                    .active(|style| style.opacity(0.8))
                    .on_click(cx.listener(|this, _, _, cx| this.advance_user_input(cx)))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.advance_user_input(cx);
                            cx.stop_propagation();
                        }
                    }))
            })
            .child(if is_last {
                tr!("user_input.submit")
            } else {
                tr!("user_input.next")
            });

        let progress = (pending.questions.len() > 1).then(|| {
            div()
                .h(px(18.0))
                .px(px(6.0))
                .rounded(px(5.0))
                .bg(theme.overlay)
                .flex()
                .items_center()
                .text_size(sp(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text_tertiary)
                .child(tr!(
                    "user_input.progress",
                    current = question_index + 1,
                    total = pending.questions.len()
                ))
        });

        div().flex_none().px(px(20.0)).pb(px(8.0)).child(
            div()
                .id(SharedString::from(format!("user-input-{request_id}")))
                .w_full()
                .max_w(px(CONTENT_MAX_WIDTH))
                .mx_auto()
                .px(px(14.0))
                .pt(px(12.0))
                .pb(px(10.0))
                .rounded(px(13.0))
                .border_1()
                .border_color(theme.border)
                .bg(theme.composer)
                .tab_index(0)
                .tab_group()
                .tab_stop(false)
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(sp(12.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text_tertiary)
                                .child(SharedString::from(question.header.clone())),
                        )
                        .children(progress),
                )
                .child(
                    div()
                        .mt(px(5.0))
                        .text_size(sp(13.0))
                        .line_height(sp(18.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(theme.text)
                        .whitespace_normal()
                        .child(SharedString::from(question.question.clone())),
                )
                .children((!question.options.is_empty()).then_some(options))
                .child(
                    div()
                        .mt(px(if question.options.is_empty() {
                            9.0
                        } else {
                            4.0
                        }))
                        .h(px(34.0))
                        .px(px(10.0))
                        .rounded(px(8.0))
                        .border_1()
                        .border_color(if has_custom {
                            theme.accent.opacity(0.34)
                        } else {
                            theme.border.opacity(0.0)
                        })
                        .bg(if has_custom {
                            theme.accent.opacity(0.06)
                        } else {
                            theme.overlay
                        })
                        .flex()
                        .items_center()
                        .gap(px(7.0))
                        .text_size(sp(12.5))
                        .line_height(sp(16.0))
                        .child(icon(
                            "icons/pencil.svg",
                            11.0,
                            if has_custom {
                                theme.accent
                            } else {
                                theme.text_ghost
                            },
                        ))
                        .child(self.user_input_answer.clone()),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .flex()
                        .items_center()
                        .children(back)
                        .child(div().flex_1())
                        .child(continue_button),
                ),
        )
    }

    fn render_computer_permission(
        &self,
        permission: &PendingComputerApproval,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = Theme::current(cx);
        let target = &permission.target;
        let mut buttons = div().mt(px(12.0)).flex().items_center().gap(px(8.0));
        let mut options = vec![
            ("task", tr!("computer_use.allow_for_task"), true),
            ("deny", tr!("common.deny"), false),
        ];
        if target.persistable() {
            options.insert(1, ("always", tr!("computer_use.always_allow_app"), false));
        }
        for (decision, label, primary) in options {
            buttons = buttons.child(
                div()
                    .id(SharedString::from(format!(
                        "computer-permission-{}-{decision}",
                        permission.request.call_id
                    )))
                    .h(px(29.0))
                    .px(px(13.0))
                    .rounded(px(7.0))
                    .flex()
                    .items_center()
                    .cursor_default()
                    .text_size(sp(12.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .when(primary, |element| {
                        element
                            .bg(theme.inverse)
                            .text_color(theme.on_inverse)
                            .hover(|element| element.opacity(0.9))
                    })
                    .when(!primary, |element| {
                        element
                            .border_1()
                            .border_color(theme.border_strong)
                            .text_color(theme.text_secondary)
                            .hover(|element| element.bg(theme.overlay).text_color(theme.text))
                    })
                    .active(|element| element.opacity(0.8))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.respond_computer_permission(decision, cx);
                    })),
            );
        }

        div().px(px(20.0)).pb(px(8.0)).child(
            div()
                .w_full()
                .max_w(px(CONTENT_MAX_WIDTH))
                .mx_auto()
                .p(px(13.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(theme.warning.opacity(0.5))
                .bg(theme.raised)
                .shadow_md()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(9.0))
                        .child(icon("icons/globe.svg", 14.0, theme.warning))
                        .child(
                            div()
                                .text_size(sp(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child(tr!("computer_use.allow_control", app = &target.app_name)),
                        ),
                )
                .child(
                    div()
                        .mt(px(7.0))
                        .text_size(sp(12.5))
                        .line_height(sp(14.0))
                        .text_color(theme.text_secondary)
                        .child(tr!("computer_use.screenshot_shared")),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .p(px(9.0))
                        .rounded(px(8.0))
                        .bg(theme.inset)
                        .child(
                            div()
                                .text_size(sp(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .truncate()
                                .child(SharedString::from(target.window_title.clone())),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .text_size(sp(12.5))
                                .text_color(theme.text_secondary)
                                .child(SharedString::from(permission.request.summary())),
                        )
                        .when(permission.sensitive, |element| {
                            element.child(
                                div()
                                    .mt(px(5.0))
                                    .text_size(sp(12.5))
                                    .text_color(theme.warning)
                                    .child(tr!("computer_use.sensitive_action")),
                            )
                        }),
                )
                .child(
                    div()
                        .mt(px(7.0))
                        .text_size(sp(12.5))
                        .text_color(theme.text_tertiary)
                        .child(if target.persistable() {
                            tr!("computer_use.bundle_id", id = &target.bundle_id)
                        } else {
                            tr!("computer_use.no_bundle_id")
                        }),
                )
                .child(buttons),
        )
    }

    pub(super) fn render_computer_use_overlay(&self, cx: &mut Context<Self>) -> Option<Div> {
        let previews = self
            .selected_runtime()?
            .computer_use_previews
            .iter()
            .filter(|state| state.visible && state.phase != ComputerUsePhase::AwaitingApproval)
            .collect::<Vec<_>>();
        if previews.is_empty() {
            return None;
        }
        let theme = Theme::current(cx);
        let stack_x_offset = 14.0;
        let stack_y_offset = 24.0;
        let deepest_x_offset = (previews.len().saturating_sub(1) as f32) * stack_x_offset;
        let deepest_y_offset = (previews.len().saturating_sub(1) as f32) * stack_y_offset;
        let top_index = previews.len() - 1;
        let cards = previews
            .into_iter()
            .enumerate()
            .filter_map(|(index, state)| {
                let target = state.target.as_ref()?;
                let window_id = target.window_id;
                let app_name = target.app_name.clone();
                let app_initial = app_name.chars().next().unwrap_or('W').to_string();
                let title = target.window_title.clone();
                let screenshot = state.screenshot.clone();
                let active = state.phase == ComputerUsePhase::Running;
                let failed = state.phase == ComputerUsePhase::Failed;
                let is_top = index == top_index;
                let depth = (top_index - index) as f32;
                let x_offset = depth * stack_x_offset;
                let y_offset = depth * stack_y_offset;
                let status_color = if failed {
                    theme.danger
                } else if active {
                    theme.warning
                } else {
                    theme.accent
                };
                let status = if failed {
                    tr!("computer_use.stopped")
                } else if active {
                    tr!("computer_use.controlling")
                } else {
                    tr!("computer_use.captured")
                };

                Some(
                    div()
                        .id(SharedString::from(format!(
                            "computer-use-preview-{window_id}"
                        )))
                        .absolute()
                        .right(px(x_offset))
                        .bottom(px(y_offset))
                        .w(px(304.0))
                        .h(px(220.0))
                        .p(px(6.0))
                        .rounded(px(16.0))
                        .overflow_hidden()
                        .border_1()
                        .border_color(if is_top {
                            theme.border_strong
                        } else {
                            theme.border
                        })
                        .bg(theme.raised)
                        .shadow_lg()
                        .cursor_default()
                        .when(!is_top, |element| element.opacity(0.96))
                        .child(
                            div()
                                .h(px(38.0))
                                .px(px(5.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .child(
                                    div()
                                        .w(px(27.0))
                                        .h(px(27.0))
                                        .rounded(px(7.0))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .bg(theme.overlay_strong)
                                        .text_size(sp(12.5))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.text_secondary)
                                        .child(SharedString::from(app_initial)),
                                )
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .text_size(sp(12.5))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text)
                                                .truncate()
                                                .child(SharedString::from(app_name)),
                                        )
                                        .child(
                                            div()
                                                .mt(px(1.0))
                                                .text_size(sp(12.5))
                                                .text_color(theme.text_tertiary)
                                                .truncate()
                                                .child(SharedString::from(title)),
                                        ),
                                )
                                .when(is_top, |element| {
                                    element.child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "computer-use-preview-action-{window_id}"
                                            )))
                                            .h(px(27.0))
                                            .px(px(10.0))
                                            .rounded(px(7.0))
                                            .border_1()
                                            .border_color(theme.border_strong)
                                            .flex()
                                            .items_center()
                                            .cursor_default()
                                            .text_size(sp(12.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(if active {
                                                theme.danger
                                            } else {
                                                theme.text_secondary
                                            })
                                            .hover(|element| element.bg(theme.overlay))
                                            .active(|element| element.opacity(0.8))
                                            .child(if active {
                                                tr!("computer_use.take_control")
                                            } else {
                                                tr!("common.close")
                                            })
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.stop_propagation();
                                                if active {
                                                    this.cancel_turn(cx);
                                                } else {
                                                    this.dismiss_computer_use(window_id, cx);
                                                }
                                            })),
                                    )
                                }),
                        )
                        .child(
                            div()
                                .relative()
                                .h(px(170.0))
                                .w_full()
                                .rounded(px(11.0))
                                .overflow_hidden()
                                .bg(rgb(0x101010))
                                .when_some(screenshot, |element, screenshot| {
                                    element.child(
                                        img(screenshot)
                                            .w_full()
                                            .h_full()
                                            .object_fit(ObjectFit::Contain),
                                    )
                                })
                                .when(state.screenshot.is_none(), |element| {
                                    element.child(
                                        div()
                                            .absolute()
                                            .inset_0()
                                            .flex()
                                            .flex_col()
                                            .items_center()
                                            .justify_center()
                                            .gap(px(9.0))
                                            .child(
                                                div()
                                                    .w(px(34.0))
                                                    .h(px(23.0))
                                                    .rounded(px(5.0))
                                                    .border_1()
                                                    .border_color(theme.text_tertiary)
                                                    .child(
                                                        div()
                                                            .mt(px(4.0))
                                                            .ml(px(25.0))
                                                            .w(px(3.0))
                                                            .h(px(3.0))
                                                            .rounded_full()
                                                            .bg(status_color),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_size(sp(12.5))
                                                    .text_color(theme.text_tertiary)
                                                    .child(tr!("computer_use.preparing_preview")),
                                            ),
                                    )
                                })
                                .child(
                                    div()
                                        .absolute()
                                        .top(px(8.0))
                                        .left(px(8.0))
                                        .h(px(24.0))
                                        .px(px(8.0))
                                        .rounded_full()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .bg(theme.canvas.opacity(0.86))
                                        .border_1()
                                        .border_color(theme.border)
                                        .child(
                                            div()
                                                .w(px(6.0))
                                                .h(px(6.0))
                                                .rounded_full()
                                                .bg(status_color),
                                        )
                                        .child(
                                            div()
                                                .text_size(sp(12.5))
                                                .font_weight(FontWeight::MEDIUM)
                                                .text_color(theme.text)
                                                .child(status),
                                        ),
                                ),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.bring_computer_use_to_front(window_id, cx);
                        })),
                )
            })
            .collect::<Vec<_>>();

        Some(
            div()
                .absolute()
                .right(px(16.0))
                .bottom(px(82.0))
                .w(px(304.0 + deepest_x_offset))
                .h(px(220.0 + deepest_y_offset))
                .children(cards),
        )
    }

    // ── Composer ───────────────────────────────────────────────────────────

    pub(super) fn render_provider_model_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::current(cx);
        let session = self.selected_session();
        let provider = session
            .map(|session| session.provider)
            .unwrap_or(self.state.last_provider);
        let selected_model = session
            .and_then(|session| self.model_for_session(session))
            .or(self.state.last_model.as_deref())
            .or_else(|| {
                self.provider_probe(provider)
                    .and_then(ProviderProbe::preferred_model)
                    .map(|m| m.id.as_str())
            });
        let selected_model_name = self.model_display_name(provider, selected_model);
        let locked_provider: Option<ProviderKind> = None;
        let picker_enabled = session
            .map(|session| session.can_choose_model(provider))
            .unwrap_or(true);

        let current_model_meta = self.provider_probe(provider).and_then(|probe| {
            let name = selected_model.as_deref().unwrap_or("");
            probe.models.iter().find(|m| m.id == name).cloned()
        });

        let selected_effort = session
            .and_then(|s| s.reasoning_effort.as_deref())
            .or(self.state.last_reasoning_effort.as_deref())
            .filter(|selected| {
                current_model_meta.as_ref().map_or(false, |m| {
                    m.reasoning_efforts.iter().any(|opt| opt.id == *selected)
                })
            })
            .or_else(|| {
                current_model_meta
                    .as_ref()
                    .and_then(|m| m.default_reasoning_effort.as_deref())
            })
            .or_else(|| {
                current_model_meta
                    .as_ref()
                    .and_then(|m| m.reasoning_efforts.first().map(|opt| opt.id.as_str()))
            })
            .map(str::to_owned);

        let effort_label = selected_effort.as_deref().and_then(|selected| {
            current_model_meta
                .as_ref()
                .and_then(|m| m.reasoning_efforts.iter().find(|opt| opt.id == selected))
                .map(|opt| opt.label.clone())
        });

        let selected_tier = session
            .and_then(|s| s.service_tier.as_deref())
            .or(self.state.last_service_tier.as_deref())
            .filter(|selected| {
                *selected == "default"
                    || current_model_meta.as_ref().map_or(false, |m| {
                        m.service_tiers.iter().any(|opt| opt.id == *selected)
                    })
            })
            .or_else(|| {
                current_model_meta
                    .as_ref()
                    .and_then(|m| m.default_service_tier.as_deref())
            })
            .unwrap_or("default")
            .to_owned();

        let selected_mode = session
            .map(|session| session.runtime_mode)
            .unwrap_or(self.state.last_runtime_mode);

        let agent_presets = if provider == ProviderKind::DeepSeek {
            self.provider_probe(ProviderKind::DeepSeek)
                .map(|probe| probe.agent_presets.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        let selected_preset_id = if provider == ProviderKind::DeepSeek {
            session
                .and_then(|s| self.agent_preset_for_session(s))
                .or_else(|| {
                    self.provider_probe(ProviderKind::DeepSeek)
                        .and_then(ProviderProbe::preferred_agent_preset)
                        .map(|preset| preset.id.clone())
                })
        } else {
            None
        };

        if !picker_enabled {
            let label = if let Some(ref effort) = effort_label {
                format!("{selected_model_name}  {effort}")
            } else {
                selected_model_name
            };
            return div()
                .h(px(24.0))
                .px(px(7.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(provider_mark(
                    &theme,
                    provider,
                    10.5,
                    provider_color(&theme, provider).opacity(0.9),
                ))
                .child(
                    div()
                        .max_w(px(240.0))
                        .truncate()
                        .text_color(theme.text_secondary)
                        .child(SharedString::from(label)),
                )
                .child(div().text_color(theme.text_ghost).child("·"))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .child(icon(
                            selected_mode.icon(),
                            11.5,
                            runtime_mode_color(&theme, selected_mode),
                        ))
                        .child(
                            div()
                                .text_color(theme.text_secondary)
                                .child(selected_mode.label()),
                        ),
                )
                .into_any_element();
        }

        let search_query = self.model_search.read(cx).content().to_owned();
        let normalized_query = search_query.trim().to_ascii_lowercase();
        let searching = !normalized_query.is_empty();
        let selected_tab = self.model_picker_tab;
        let selected_model = selected_model.map(str::to_owned);
        let probes = self.probes.clone();
        let disabled_providers = self.state.disabled_providers.clone();
        let pending_discoveries = self.provider_model_discoveries_pending.clone();
        let recents = self.state.recent_models.clone();
        let favorites = self.state.favorite_models.clone();
        let weak = cx.entity().downgrade();
        let search = self.model_search.clone();
        let search_focus = search.read(cx).focus_handle(cx);
        let empty_focus = self.model_picker_empty_focus.clone();
        let no_providers = self.model_picker_has_no_providers();

        let handle = {
            let reset_weak = weak.clone();
            let reset_search = search.clone();
            let picker_focus = search_focus.clone();
            let empty_picker_focus = empty_focus.clone();
            self.menu_handle_with(MODEL_PICKER_MENU_ID, cx, move |open, window, cx| {
                // The empty state draws no filter field, so the handle the
                // deferred focus below targets depends on which body opened.
                let mut empty = false;
                let _ = reset_weak.update(cx, |this, cx| {
                    if open {
                        empty = this.model_picker_has_no_providers();
                        let provider = this
                            .selected_session()
                            .map(|session| session.provider)
                            .unwrap_or(this.state.last_provider);
                        // A draft can sit on a provider that was since switched
                        // off; open onto the first usable provider instead of a
                        // tab whose rows the filter would leave empty.
                        let locked = this
                            .selected_session()
                            .is_some_and(|session| !session.messages.is_empty());
                        let provider =
                            if !locked && this.state.disabled_providers.contains(&provider) {
                                ProviderKind::ALL
                                    .into_iter()
                                    .find(|kind| this.provider_enabled(*kind))
                                    .unwrap_or(provider)
                            } else {
                                provider
                            };
                        this.model_picker_tab = ModelPickerTab::Recents;
                        this.model_picker_submenu = ModelPickerSubmenu::None;
                        // Opening re-runs the tab's catalog discovery so models
                        // authored since launch appear without a restart; the
                        // other rails refresh when selected, not all at once.
                        this.refresh_provider_model_discovery(provider);
                        this.model_picker_highlight = None;
                        reset_search.update(cx, |search, cx| search.clear(cx));
                        this.reveal_selected_picker_model();
                    } else {
                        this.model_picker_submenu = ModelPickerSubmenu::None;
                        let focus_handle = this.composer.read(cx).focus();
                        window.focus(&focus_handle, cx);
                    }
                    cx.notify();
                });
                if open {
                    // The panel is deferred, so its input joins the dispatch
                    // tree only after the deferred draw — same two-frame wait
                    // the menus need before they can take focus. The reveal is
                    // re-issued here too: a parked scroll request resolves
                    // against the viewport bounds of the *previous* paint, so
                    // on the container's first-ever paint it reads a zeroed
                    // viewport, lands wrong, and is consumed. By this frame
                    // the geometry has settled.
                    let focus = if empty {
                        empty_picker_focus.clone()
                    } else {
                        picker_focus.clone()
                    };
                    window.focus(&focus, cx);
                }
            })
        };

        // Only while the panel is open: this clones every installed provider's
        // model list, and the closed picker is on the composer's every frame.
        // Built out here rather than in the body so the key handler and the
        // rendered rows index one ordering and cannot disagree about what
        // `enter` selects.
        let available_models = Rc::new(if handle.is_open() {
            visible_picker_models(
                &probes,
                &recents,
                &favorites,
                &disabled_providers,
                locked_provider,
                selected_tab,
                &normalized_query,
            )
        } else {
            Vec::new()
        });
        let highlight = self
            .model_picker_highlight
            .filter(|index| *index < available_models.len());
        let scroll = self.model_picker_scroll.clone();
        let scrollbar_state = self.model_picker_scrollbar.clone();
        let tabs_scroll = self.model_picker_tabs_scroll.clone();
        let tabs_scrollbar_state = self.model_picker_tabs_scrollbar.clone();

        let trigger_label = if let Some(ref effort) = effort_label {
            format!("{selected_model_name}  {effort}")
        } else {
            selected_model_name.clone()
        };

        // With nothing to pick from, naming a model the app cannot run would
        // be a lie. The chip says so instead, and stays a trigger because the
        // panel behind it is where the fix lives. Icon plus wording carry the
        // state on their own, so the warning tint is never the only signal.
        let trigger = if no_providers {
            MenuChip::new("composer-provider-model")
                .icon("icons/alert.svg", theme.warning)
                .label(tr!("models.no_providers"))
        } else {
            MenuChip::new("composer-provider-model")
                .provider(
                    &theme,
                    provider,
                    provider_color(&theme, provider).opacity(0.9),
                )
                .label(trigger_label)
                .trailing_tag(
                    selected_mode.icon(),
                    runtime_mode_color(&theme, selected_mode),
                    selected_mode.label(),
                )
        }
        .max_w(px(280.0));

        let footer_model_meta = current_model_meta.clone();
        let footer_effort = selected_effort.clone();
        let footer_tier = selected_tier.clone();
        let footer_mode = selected_mode;
        let footer_presets = agent_presets.clone();
        let footer_preset_id = selected_preset_id.clone();
        let footer_submenu = self.model_picker_submenu;

        popover(
            trigger.caret(true).selected(handle.is_open()),
            &handle,
            MenuAlign::AboveLeft,
            move |popover, _window, _cx| {
                let popover = popover.clone();
                let available_models = available_models.clone();
                let footer_model_meta = footer_model_meta.clone();
                let footer_effort = footer_effort.clone();
                let footer_tier = footer_tier.clone();
                let footer_presets = footer_presets.clone();
                let footer_preset_id = footer_preset_id.clone();
                let footer_submenu = footer_submenu;
                let tabs_scroll = tabs_scroll.clone();
                let tabs_scrollbar_state = tabs_scrollbar_state.clone();

                if no_providers {
                    return model_picker_empty_state(&theme, &empty_focus, popover, weak.clone());
                }

                let rail_tabs = visible_picker_tabs(&probes, &disabled_providers, locked_provider);
                let show_tabs = rail_tabs.len() > 1 && !searching;
                let mut tabs_list = div()
                    .id("model-picker-tabs-bar")
                    .w_full()
                    .min_w_0()
                    .px(px(8.0))
                    .py(px(5.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .overflow_x_scroll()
                    .track_scroll(&tabs_scroll);

                let recents_selected = selected_tab == ModelPickerTab::Recents;
                let recents_weak = weak.clone();
                tabs_list = tabs_list.child(
                    div()
                        .id("model-tab-recents")
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(6.0))
                        .cursor_default()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .text_size(sp(11.5))
                        .when(recents_selected, |element| {
                            element
                                .bg(theme.overlay_strong)
                                .text_color(theme.text)
                                .font_weight(FontWeight::SEMIBOLD)
                        })
                        .when(!recents_selected, |element| {
                            element
                                .text_color(theme.text_tertiary)
                                .hover(|el| el.bg(theme.overlay).text_color(theme.text))
                        })
                        .child(icon(
                            "icons/history.svg",
                            12.0,
                            if recents_selected {
                                theme.text
                            } else {
                                theme.text_tertiary
                            },
                        ))
                        .child("Recents")
                        .on_click(move |_, _, cx| {
                            let _ = recents_weak.update(cx, |this, cx| {
                                this.select_model_picker_tab(ModelPickerTab::Recents, cx);
                            });
                        }),
                );

                let favorites_selected = selected_tab == ModelPickerTab::Favorites;
                let favorite_weak = weak.clone();
                tabs_list = tabs_list.child(
                    div()
                        .id("model-tab-favorites")
                        .flex_shrink_0()
                        .whitespace_nowrap()
                        .px(px(8.0))
                        .py(px(3.0))
                        .rounded(px(6.0))
                        .cursor_default()
                        .flex()
                        .items_center()
                        .gap(px(4.0))
                        .text_size(sp(11.5))
                        .when(favorites_selected, |element| {
                            element
                                .bg(theme.overlay_strong)
                                .text_color(theme.text)
                                .font_weight(FontWeight::SEMIBOLD)
                        })
                        .when(!favorites_selected, |element| {
                            element
                                .text_color(theme.text_tertiary)
                                .hover(|el| el.bg(theme.overlay).text_color(theme.text))
                        })
                        .child(icon(
                            "icons/star.svg",
                            12.0,
                            if favorites_selected {
                                theme.text
                            } else {
                                theme.text_tertiary
                            },
                        ))
                        .child("Favorites")
                        .on_click(move |_, _, cx| {
                            let _ = favorite_weak.update(cx, |this, cx| {
                                this.select_model_picker_tab(ModelPickerTab::Favorites, cx);
                            });
                        }),
                );

                for &tab in &rail_tabs {
                    let ModelPickerTab::Provider(kind) = tab else {
                        continue;
                    };
                    let selected = selected_tab == ModelPickerTab::Provider(kind);
                    let tab_weak = weak.clone();
                    tabs_list = tabs_list.child(
                        div()
                            .id(SharedString::from(format!("model-tab-{}", kind.id())))
                            .flex_shrink_0()
                            .whitespace_nowrap()
                            .px(px(8.0))
                            .py(px(3.0))
                            .rounded(px(6.0))
                            .cursor_default()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .text_size(sp(11.5))
                            .when(selected, |element| {
                                element
                                    .bg(theme.overlay_strong)
                                    .text_color(theme.text)
                                    .font_weight(FontWeight::SEMIBOLD)
                            })
                            .when(!selected, |element| {
                                element
                                    .text_color(theme.text_tertiary)
                                    .hover(|el| el.bg(theme.overlay).text_color(theme.text))
                            })
                            .child(provider_mark(
                                &theme,
                                kind,
                                11.5,
                                provider_color(&theme, kind).opacity(if selected {
                                    1.0
                                } else {
                                    0.82
                                }),
                            ))
                            .child(SharedString::from(kind.short_name()))
                            .on_click(move |_, _, cx| {
                                let _ = tab_weak.update(cx, |this, cx| {
                                    this.select_model_picker_tab(
                                        ModelPickerTab::Provider(kind),
                                        cx,
                                    );
                                });
                            }),
                    );
                }

                let tabs_bar = div()
                    .id("model-picker-tabs-wrapper")
                    .relative()
                    .w_full()
                    .min_w_0()
                    .border_b_1()
                    .border_color(theme.border)
                    .flex_none()
                    .child(tabs_list)
                    .child(scrollbar::horizontal(&tabs_scroll, &tabs_scrollbar_state));

                let search_input = div()
                    .h(px(40.0))
                    .px(px(12.0))
                    .border_b_1()
                    .border_color(theme.border)
                    .flex_none()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(icon("icons/search.svg", 14.0, theme.text_tertiary))
                    .child(div().flex_1().min_w_0().child(search.clone()));

                let mut rows = div()
                    .id("model-picker-list")
                    .size_full()
                    .overflow_y_scroll()
                    .track_scroll(&scroll)
                    .p(px(6.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0));
                if available_models.is_empty() {
                    let label = if searching {
                        tr!("models.none_found")
                    } else if selected_tab == ModelPickerTab::Recents {
                        tr!("models.recent_hint")
                    } else if selected_tab == ModelPickerTab::Favorites {
                        tr!("models.favorite_hint")
                    } else if matches!(
                        selected_tab,
                        ModelPickerTab::Provider(provider)
                            if pending_discoveries.contains(&provider)
                    ) {
                        tr!("models.loading")
                    } else {
                        tr!("models.none_reported")
                    };
                    rows = rows.child(
                        div()
                            .py(px(24.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(sp(12.0))
                            .text_color(theme.text_ghost)
                            .child(label),
                    );
                }

                for (row_index, (kind, model)) in available_models.iter().enumerate() {
                    let kind = *kind;
                    let is_selected =
                        kind == provider && selected_model.as_deref() == Some(model.id.as_str());
                    let is_highlighted = highlight == Some(row_index);
                    let is_favorite = favorites
                        .iter()
                        .any(|favorite| favorite.provider == kind && favorite.model == model.id);
                    let model_id = model.id.clone();
                    let select_weak = weak.clone();
                    let select_popover = popover.clone();
                    let favorite_model_id = model.id.clone();
                    let favorite_weak = weak.clone();
                    let subtitle = model_picker_subtitle(kind, model.sub_provider.as_deref());
                    rows = rows.child(
                        div()
                            .id(SharedString::from(format!(
                                "model-row-{}-{}",
                                kind.id(),
                                model.id
                            )))
                            .h(px(34.0))
                            .w_full()
                            .px(px(10.0))
                            .rounded(px(6.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap(px(8.0))
                            .cursor_default()
                            .border_1()
                            .border_color(gpui::transparent_black())
                            .when(is_selected, |element| element.bg(theme.overlay_strong))
                            .when(is_highlighted, |element| {
                                element.bg(theme.overlay).border_color(theme.accent)
                            })
                            .hover(|element| element.bg(theme.overlay))
                            .active(|element| element.opacity(0.85))
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(sp(12.5))
                                            .font_weight(if is_selected {
                                                FontWeight::SEMIBOLD
                                            } else {
                                                FontWeight::MEDIUM
                                            })
                                            .text_color(theme.text)
                                            .child(SharedString::from(model.name.clone())),
                                    )
                                    .when(
                                        selected_tab == ModelPickerTab::Recents
                                            || selected_tab == ModelPickerTab::Favorites
                                            || searching
                                            || model.sub_provider.is_some(),
                                        |element| {
                                            element.child(
                                                div()
                                                    .flex_shrink_0()
                                                    .flex()
                                                    .items_center()
                                                    .gap(px(4.0))
                                                    .child(provider_mark(
                                                        &theme,
                                                        kind,
                                                        9.5,
                                                        provider_color(&theme, kind).opacity(0.8),
                                                    ))
                                                    .child(
                                                        div()
                                                            .truncate()
                                                            .text_size(sp(11.0))
                                                            .text_color(theme.text_tertiary)
                                                            .child(SharedString::from(subtitle)),
                                                    ),
                                            )
                                        },
                                    ),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .when(is_selected, |element| {
                                        element.child(icon("icons/check.svg", 12.0, theme.accent))
                                    })
                                    .child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "favorite-model-{}-{}",
                                                kind.id(),
                                                model.id
                                            )))
                                            .w(px(22.0))
                                            .h(px(22.0))
                                            .rounded(px(4.0))
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .hover(|element| element.bg(theme.overlay_strong))
                                            .child(icon(
                                                if is_favorite {
                                                    "icons/star-filled.svg"
                                                } else {
                                                    "icons/star.svg"
                                                },
                                                12.0,
                                                if is_favorite {
                                                    theme.favorite
                                                } else {
                                                    theme.text_ghost
                                                },
                                            ))
                                            .on_click(move |_, _, cx| {
                                                cx.stop_propagation();
                                                let _ = favorite_weak.update(cx, |this, cx| {
                                                    this.toggle_favorite_model(
                                                        kind,
                                                        favorite_model_id.clone(),
                                                        cx,
                                                    );
                                                });
                                            }),
                                    ),
                            )
                            .on_click(move |_, window, cx| {
                                let _ = select_weak.update(cx, |this, cx| {
                                    this.choose_model(kind, model_id.clone(), cx);
                                });
                                select_popover.close(window, cx);
                            }),
                    );
                }

                let next_models = available_models.clone();
                let previous_models = available_models.clone();
                let confirm_models = available_models.clone();
                let next_weak = weak.clone();
                let previous_weak = weak.clone();
                let next_tab_weak = weak.clone();
                let previous_tab_weak = weak.clone();
                let confirm_weak = weak.clone();
                let confirm_popover = popover.clone();
                let mut footer_controls = Vec::new();
                let active_submenu = footer_submenu;
                let submenu_bg = if theme.is_dark {
                    Hsla {
                        a: 1.0,
                        l: 0.14_f32.max(theme.elevated.l),
                        ..theme.elevated
                    }
                } else {
                    Hsla {
                        a: 1.0,
                        l: 0.98,
                        ..theme.elevated
                    }
                };

                // 1. Reasoning menu button
                if let Some(ref model_meta) = footer_model_meta {
                    if !model_meta.reasoning_efforts.is_empty() {
                        let is_open = active_submenu == ModelPickerSubmenu::Reasoning;
                        let reasoning_label = footer_effort
                            .as_deref()
                            .and_then(|id| {
                                model_meta
                                    .reasoning_efforts
                                    .iter()
                                    .find(|o| o.id == id)
                                    .map(|o| o.label.as_str())
                            })
                            .or_else(|| model_meta.default_reasoning_effort.as_deref())
                            .unwrap_or("Off");

                        let weak_reasoning = weak.clone();
                        footer_controls.push(
                            div()
                                .id("model-picker-reasoning-trigger")
                                .flex_shrink_0()
                                .h(px(26.0))
                                .px(px(8.0))
                                .rounded(px(6.0))
                                .cursor_default()
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .border_1()
                                .border_color(if is_open {
                                    theme.accent
                                } else {
                                    theme.border_strong
                                })
                                .bg(if is_open {
                                    theme.overlay_strong.opacity(1.8)
                                } else {
                                    theme.overlay_strong
                                })
                                .hover(|el| el.bg(theme.overlay_strong.opacity(1.8)))
                                .child(icon("icons/sparkle.svg", 11.0, theme.accent))
                                .child(
                                    div()
                                        .text_size(sp(11.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.text_secondary)
                                        .child(tr!("models.reasoning")),
                                )
                                .child(
                                    div()
                                        .text_size(sp(11.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.text)
                                        .child(SharedString::from(reasoning_label.to_owned())),
                                )
                                .child(icon("icons/chevron-down.svg", 9.0, theme.text_secondary))
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    let _ = weak_reasoning.update(cx, |this, cx| {
                                        this.toggle_model_picker_submenu(
                                            ModelPickerSubmenu::Reasoning,
                                            cx,
                                        );
                                    });
                                })
                                .into_any_element(),
                        );
                    }

                    // 2. Service Tier menu button (if any)
                    if !model_meta.service_tiers.is_empty() {
                        let is_open = active_submenu == ModelPickerSubmenu::ServiceTier;
                        let tier_label = if footer_tier == "default" {
                            tr!("models.standard")
                        } else {
                            model_meta
                                .service_tiers
                                .iter()
                                .find(|o| o.id == footer_tier)
                                .map(|o| o.label.clone())
                                .unwrap_or_else(|| footer_tier.clone())
                        };

                        let weak_tier = weak.clone();
                        footer_controls.push(
                            div()
                                .id("model-picker-tier-trigger")
                                .flex_shrink_0()
                                .h(px(26.0))
                                .px(px(8.0))
                                .rounded(px(6.0))
                                .cursor_default()
                                .flex()
                                .items_center()
                                .gap(px(5.0))
                                .border_1()
                                .border_color(if is_open {
                                    theme.accent
                                } else {
                                    theme.border_strong
                                })
                                .bg(if is_open {
                                    theme.overlay_strong.opacity(1.8)
                                } else {
                                    theme.overlay_strong
                                })
                                .hover(|el| el.bg(theme.overlay_strong.opacity(1.8)))
                                .child(icon("icons/zap.svg", 11.0, theme.accent))
                                .child(
                                    div()
                                        .text_size(sp(11.0))
                                        .font_weight(FontWeight::MEDIUM)
                                        .text_color(theme.text_secondary)
                                        .child(tr!("models.service_tier")),
                                )
                                .child(
                                    div()
                                        .text_size(sp(11.0))
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(theme.text)
                                        .child(SharedString::from(tier_label)),
                                )
                                .child(icon("icons/chevron-down.svg", 9.0, theme.text_secondary))
                                .on_click(move |_, _, cx| {
                                    cx.stop_propagation();
                                    let _ = weak_tier.update(cx, |this, cx| {
                                        this.toggle_model_picker_submenu(
                                            ModelPickerSubmenu::ServiceTier,
                                            cx,
                                        );
                                    });
                                })
                                .into_any_element(),
                        );
                    }
                }

                // 3. Permissions / RuntimeMode menu button
                let is_perms_open = active_submenu == ModelPickerSubmenu::Permissions;
                let mode_color = runtime_mode_color(&theme, footer_mode);
                let weak_perms = weak.clone();
                footer_controls.push(
                    div()
                        .id("model-picker-perms-trigger")
                        .flex_shrink_0()
                        .h(px(26.0))
                        .px(px(8.0))
                        .rounded(px(6.0))
                        .cursor_default()
                        .flex()
                        .items_center()
                        .gap(px(5.0))
                        .border_1()
                        .border_color(if is_perms_open {
                            mode_color
                        } else {
                            theme.border_strong
                        })
                        .bg(if is_perms_open {
                            theme.overlay_strong.opacity(1.8)
                        } else {
                            theme.overlay_strong
                        })
                        .hover(|el| el.bg(theme.overlay_strong.opacity(1.8)))
                        .child(icon(footer_mode.icon(), 11.0, mode_color))
                        .child(
                            div()
                                .text_size(sp(11.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_secondary)
                                .child(tr!("models.permissions")),
                        )
                        .child(
                            div()
                                .text_size(sp(11.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(theme.text)
                                .child(footer_mode.label()),
                        )
                        .child(icon("icons/chevron-down.svg", 9.0, theme.text_secondary))
                        .on_click(move |_, _, cx| {
                            cx.stop_propagation();
                            let _ = weak_perms.update(cx, |this, cx| {
                                this.toggle_model_picker_submenu(
                                    ModelPickerSubmenu::Permissions,
                                    cx,
                                );
                            });
                        })
                        .into_any_element(),
                );

                // 4. Agent Presets (DeepSeek only)
                if !footer_presets.is_empty() {
                    let is_preset_open = active_submenu == ModelPickerSubmenu::AgentPreset;
                    let preset_label = footer_preset_id
                        .as_deref()
                        .and_then(|id| {
                            footer_presets
                                .iter()
                                .find(|p| p.id == id)
                                .map(|p| p.display_name())
                        })
                        .unwrap_or_else(|| {
                            footer_presets
                                .first()
                                .map(|p| p.display_name())
                                .unwrap_or_else(|| "Preset".to_string())
                        });

                    let weak_preset = weak.clone();
                    footer_controls.push(
                        div()
                            .id("model-picker-preset-trigger")
                            .flex_shrink_0()
                            .h(px(26.0))
                            .px(px(8.0))
                            .rounded(px(6.0))
                            .cursor_default()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .border_1()
                            .border_color(if is_preset_open {
                                theme.accent
                            } else {
                                theme.border_strong
                            })
                            .bg(if is_preset_open {
                                theme.overlay_strong.opacity(1.8)
                            } else {
                                theme.overlay_strong
                            })
                            .hover(|el| el.bg(theme.overlay_strong.opacity(1.8)))
                            .child(icon("icons/bot.svg", 11.0, theme.accent))
                            .child(
                                div()
                                    .text_size(sp(11.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text_secondary)
                                    .child("Preset"),
                            )
                            .child(
                                div()
                                    .text_size(sp(11.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(theme.text)
                                    .child(SharedString::from(preset_label)),
                            )
                            .child(icon("icons/chevron-down.svg", 9.0, theme.text_secondary))
                            .on_click(move |_, _, cx| {
                                cx.stop_propagation();
                                let _ = weak_preset.update(cx, |this, cx| {
                                    this.toggle_model_picker_submenu(
                                        ModelPickerSubmenu::AgentPreset,
                                        cx,
                                    );
                                });
                            })
                            .into_any_element(),
                    );
                }

                // Build submenu floating card when a menu is open
                let active_submenu_card: Option<AnyElement> = match active_submenu {
                    ModelPickerSubmenu::Reasoning => footer_model_meta.as_ref().map(|model_meta| {
                        let mut items = div()
                            .id("submenu-reasoning-items")
                            .w(px(280.0))
                            .max_h(px(220.0))
                            .overflow_y_scroll()
                            .p(px(4.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0));

                        for option in &model_meta.reasoning_efforts {
                            let is_selected = footer_effort.as_deref() == Some(option.id.as_str());
                            let opt_id = option.id.clone();
                            let opt_desc = option.description.clone();
                            let opt_label = option.label.clone();
                            let weak = weak.clone();
                            items = items.child(
                                div()
                                    .id(SharedString::from(format!("effort-item-{}", option.id)))
                                    .w_full()
                                    .px(px(8.0))
                                    .py(px(5.0))
                                    .rounded(px(5.0))
                                    .cursor_default()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .when(is_selected, |el| el.bg(theme.overlay_strong))
                                    .when(!is_selected, |el| el.hover(|el| el.bg(theme.overlay)))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(
                                                div()
                                                    .w(px(14.0))
                                                    .flex_none()
                                                    .flex()
                                                    .items_center()
                                                    .justify_center()
                                                    .when(is_selected, |el| {
                                                        el.child(icon(
                                                            "icons/check.svg",
                                                            12.0,
                                                            theme.accent,
                                                        ))
                                                    }),
                                            )
                                            .child(
                                                div()
                                                    .text_size(sp(12.0))
                                                    .font_weight(if is_selected {
                                                        FontWeight::SEMIBOLD
                                                    } else {
                                                        FontWeight::MEDIUM
                                                    })
                                                    .text_color(theme.text)
                                                    .child(SharedString::from(opt_label)),
                                            ),
                                    )
                                    .when_some(opt_desc, |el, desc| {
                                        el.child(
                                            div()
                                                .truncate()
                                                .text_size(sp(10.5))
                                                .text_color(theme.text_secondary)
                                                .child(SharedString::from(desc)),
                                        )
                                    })
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        let _ = weak.update(cx, |this, cx| {
                                            this.set_reasoning_effort(opt_id.clone(), cx);
                                            this.model_picker_submenu = ModelPickerSubmenu::None;
                                        });
                                    }),
                            );
                        }
                        div()
                            .absolute()
                            .bottom(px(46.0))
                            .left(px(8.0))
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(theme.border_strong)
                            .bg(submenu_bg)
                            .shadow_xl()
                            .child(items)
                            .into_any_element()
                    }),
                    ModelPickerSubmenu::Permissions => {
                        let mut items = div()
                            .id("submenu-perms-items")
                            .w(px(290.0))
                            .max_h(px(220.0))
                            .overflow_y_scroll()
                            .p(px(4.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0));

                        for option in RuntimeMode::ACCESS_OPTIONS {
                            let is_selected = option == footer_mode;
                            let mode_color = runtime_mode_color(&theme, option);
                            let opt_desc = option.description();
                            let opt_label = option.label();
                            let opt_icon = option.icon();
                            let weak = weak.clone();
                            items = items.child(
                                div()
                                    .id(SharedString::from(format!("mode-item-{:?}", option)))
                                    .w_full()
                                    .px(px(8.0))
                                    .py(px(5.0))
                                    .rounded(px(5.0))
                                    .cursor_default()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .when(is_selected, |el| el.bg(theme.overlay_strong))
                                    .when(!is_selected, |el| el.hover(|el| el.bg(theme.overlay)))
                                    .child(
                                        div()
                                            .w(px(14.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .when(is_selected, |el| {
                                                el.child(icon(
                                                    "icons/check.svg",
                                                    12.0,
                                                    theme.accent,
                                                ))
                                            }),
                                    )
                                    .child(icon(opt_icon, 13.0, mode_color))
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .min_w_0()
                                            .child(
                                                div()
                                                    .text_size(sp(12.0))
                                                    .font_weight(if is_selected {
                                                        FontWeight::SEMIBOLD
                                                    } else {
                                                        FontWeight::MEDIUM
                                                    })
                                                    .text_color(theme.text)
                                                    .child(opt_label),
                                            )
                                            .child(
                                                div()
                                                    .truncate()
                                                    .text_size(sp(10.5))
                                                    .text_color(theme.text_secondary)
                                                    .child(SharedString::from(opt_desc)),
                                            ),
                                    )
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        let _ = weak.update(cx, |this, cx| {
                                            this.set_runtime_mode(option, cx);
                                            this.model_picker_submenu = ModelPickerSubmenu::None;
                                        });
                                    }),
                            );
                        }
                        Some(
                            div()
                                .absolute()
                                .bottom(px(46.0))
                                .right(px(8.0))
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(theme.border_strong)
                                .bg(submenu_bg)
                                .shadow_xl()
                                .child(items)
                                .into_any_element(),
                        )
                    }
                    ModelPickerSubmenu::ServiceTier => {
                        footer_model_meta.as_ref().map(|model_meta| {
                            let mut items = div()
                                .id("submenu-tier-items")
                                .w(px(240.0))
                                .max_h(px(200.0))
                                .overflow_y_scroll()
                                .p(px(4.0))
                                .flex()
                                .flex_col()
                                .gap(px(2.0));

                            let standard_selected = footer_tier == "default";
                            let weak_std = weak.clone();
                            items = items.child(
                                div()
                                    .id("tier-item-default")
                                    .w_full()
                                    .px(px(8.0))
                                    .py(px(5.0))
                                    .rounded(px(5.0))
                                    .cursor_default()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .when(standard_selected, |el| el.bg(theme.overlay_strong))
                                    .when(!standard_selected, |el| {
                                        el.hover(|el| el.bg(theme.overlay))
                                    })
                                    .child(
                                        div()
                                            .w(px(14.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .when(standard_selected, |el| {
                                                el.child(icon(
                                                    "icons/check.svg",
                                                    12.0,
                                                    theme.accent,
                                                ))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(sp(12.0))
                                            .font_weight(if standard_selected {
                                                FontWeight::SEMIBOLD
                                            } else {
                                                FontWeight::MEDIUM
                                            })
                                            .text_color(theme.text)
                                            .child(SharedString::from(tr!("models.standard"))),
                                    )
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        let _ = weak_std.update(cx, |this, cx| {
                                            this.set_service_tier("default".to_owned(), cx);
                                            this.model_picker_submenu = ModelPickerSubmenu::None;
                                        });
                                    }),
                            );

                            for option in &model_meta.service_tiers {
                                let is_selected = footer_tier == option.id;
                                let opt_id = option.id.clone();
                                let opt_label = option.label.clone();
                                let weak_tier = weak.clone();
                                items = items.child(
                                    div()
                                        .id(SharedString::from(format!("tier-item-{}", option.id)))
                                        .w_full()
                                        .px(px(8.0))
                                        .py(px(5.0))
                                        .rounded(px(5.0))
                                        .cursor_default()
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .when(is_selected, |el| el.bg(theme.overlay_strong))
                                        .when(!is_selected, |el| {
                                            el.hover(|el| el.bg(theme.overlay))
                                        })
                                        .child(
                                            div()
                                                .w(px(14.0))
                                                .flex_none()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .when(is_selected, |el| {
                                                    el.child(icon(
                                                        "icons/check.svg",
                                                        12.0,
                                                        theme.accent,
                                                    ))
                                                }),
                                        )
                                        .child(
                                            div()
                                                .text_size(sp(12.0))
                                                .font_weight(if is_selected {
                                                    FontWeight::SEMIBOLD
                                                } else {
                                                    FontWeight::MEDIUM
                                                })
                                                .text_color(theme.text)
                                                .child(SharedString::from(opt_label)),
                                        )
                                        .on_click(move |_, _, cx| {
                                            cx.stop_propagation();
                                            let _ = weak_tier.update(cx, |this, cx| {
                                                this.set_service_tier(opt_id.clone(), cx);
                                                this.model_picker_submenu =
                                                    ModelPickerSubmenu::None;
                                            });
                                        }),
                                );
                            }
                            div()
                                .absolute()
                                .bottom(px(46.0))
                                .left(px(8.0))
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(theme.border_strong)
                                .bg(submenu_bg)
                                .shadow_xl()
                                .child(items)
                                .into_any_element()
                        })
                    }
                    ModelPickerSubmenu::AgentPreset => {
                        let mut items = div()
                            .id("submenu-preset-items")
                            .w(px(240.0))
                            .max_h(px(200.0))
                            .overflow_y_scroll()
                            .p(px(4.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0));

                        for preset in &footer_presets {
                            let is_selected =
                                footer_preset_id.as_deref() == Some(preset.id.as_str());
                            let preset_id = preset.id.clone();
                            let preset_name = preset.display_name();
                            let weak = weak.clone();
                            items = items.child(
                                div()
                                    .id(SharedString::from(format!("preset-item-{}", preset.id)))
                                    .w_full()
                                    .px(px(8.0))
                                    .py(px(5.0))
                                    .rounded(px(5.0))
                                    .cursor_default()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .when(is_selected, |el| el.bg(theme.overlay_strong))
                                    .when(!is_selected, |el| el.hover(|el| el.bg(theme.overlay)))
                                    .child(
                                        div()
                                            .w(px(14.0))
                                            .flex_none()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .when(is_selected, |el| {
                                                el.child(icon(
                                                    "icons/check.svg",
                                                    12.0,
                                                    theme.accent,
                                                ))
                                            }),
                                    )
                                    .child(
                                        div()
                                            .text_size(sp(12.0))
                                            .font_weight(if is_selected {
                                                FontWeight::SEMIBOLD
                                            } else {
                                                FontWeight::MEDIUM
                                            })
                                            .text_color(theme.text)
                                            .child(SharedString::from(preset_name)),
                                    )
                                    .on_click(move |_, _, cx| {
                                        cx.stop_propagation();
                                        let _ = weak.update(cx, |this, cx| {
                                            this.set_agent_preset(preset_id.clone(), cx);
                                            this.model_picker_submenu = ModelPickerSubmenu::None;
                                        });
                                    }),
                            );
                        }
                        Some(
                            div()
                                .absolute()
                                .bottom(px(46.0))
                                .right(px(8.0))
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(theme.border_strong)
                                .bg(submenu_bg)
                                .shadow_xl()
                                .child(items)
                                .into_any_element(),
                        )
                    }
                    ModelPickerSubmenu::None => None,
                };

                let footer = (!footer_controls.is_empty()).then(|| {
                    div()
                        .id("model-picker-footer-bar")
                        .flex_none()
                        .border_t_1()
                        .border_color(theme.border_strong)
                        .bg(if theme.is_dark {
                            Hsla {
                                a: 0.96,
                                l: 0.11_f32.max(theme.elevated_surface.l),
                                ..theme.elevated_surface
                            }
                        } else {
                            Hsla {
                                a: 0.96,
                                l: 0.94_f32.min(theme.elevated_surface.l),
                                ..theme.elevated_surface
                            }
                        })
                        .px(px(8.0))
                        .py(px(6.0))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .overflow_x_scroll()
                        .children(footer_controls)
                });

                div()
                    .relative()
                    .w(px(430.0))
                    .h(px(420.0))
                    .rounded(px(12.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border_strong)
                    .bg(theme.elevated)
                    .shadow_xl()
                    .font_family(crate::theme::active_ui_font_family())
                    .flex()
                    .flex_col()
                    .on_action(move |_: &SelectNextEntry, _, cx| {
                        let _ = next_weak.update(cx, |this, cx| {
                            this.move_model_picker_highlight("down", &next_models, cx);
                        });
                    })
                    .on_action(move |_: &SelectPreviousEntry, _, cx| {
                        let _ = previous_weak.update(cx, |this, cx| {
                            this.move_model_picker_highlight("up", &previous_models, cx);
                        });
                    })
                    .on_action(move |_: &SelectNextTab, _, cx| {
                        let _ = next_tab_weak.update(cx, |this, cx| {
                            this.cycle_model_picker_tab("down", cx);
                        });
                    })
                    .on_action(move |_: &SelectPreviousTab, _, cx| {
                        let _ = previous_tab_weak.update(cx, |this, cx| {
                            this.cycle_model_picker_tab("up", cx);
                        });
                    })
                    .on_action(move |_: &ConfirmEntry, window, cx| {
                        let _ = confirm_weak.update(cx, |this, cx| {
                            this.choose_highlighted_model(&confirm_models, cx);
                        });
                        confirm_popover.close(window, cx);
                        window.refresh();
                    })
                    .child(search_input)
                    .when(show_tabs, |card| card.child(tabs_bar))
                    .child(
                        div()
                            .w_full()
                            .flex_1()
                            .min_h_0()
                            .relative()
                            .child(rows)
                            .child(scrollbar::vertical(&scroll, &scrollbar_state)),
                    )
                    .children(footer)
                    .children(active_submenu_card)
                    .into_any_element()
            },
        )
    }

    /// Move the picker's drawn selection. Nothing is focused: the filter field
    /// keeps focus so typing continues to narrow the list.
    fn move_model_picker_highlight(
        &mut self,
        key: &str,
        models: &[(ProviderKind, ProviderModel)],
        cx: &mut Context<Self>,
    ) {
        let current = self
            .model_picker_highlight
            .filter(|index| *index < models.len());
        let Some(next) = next_picker_highlight(current, models.len(), key) else {
            return;
        };
        self.model_picker_highlight = Some(next);
        self.model_picker_scroll.scroll_to_item(next);
        cx.notify();
    }

    /// Step the sidebar rail to the adjacent usable tab, wrapping at both
    /// ends. `tab`/`shift-tab` land here from under the focused filter field,
    /// the same route the arrows take. A live query hides which tab is
    /// selected and searches across all of them, so cycling waits until the
    /// field is cleared.
    fn cycle_model_picker_tab(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.model_search.read(cx).content().trim().is_empty() {
            return;
        }
        let locked_provider: Option<ProviderKind> = None;
        let tabs = visible_picker_tabs(
            &self.probes,
            &self.state.disabled_providers,
            locked_provider,
        );
        let current = tabs.iter().position(|tab| *tab == self.model_picker_tab);
        let Some(next) = next_picker_highlight(current, tabs.len(), key) else {
            return;
        };
        self.select_model_picker_tab(tabs[next], cx);
    }

    pub(super) fn toggle_model_picker_submenu(
        &mut self,
        submenu: ModelPickerSubmenu,
        cx: &mut Context<Self>,
    ) {
        if self.model_picker_submenu == submenu {
            self.model_picker_submenu = ModelPickerSubmenu::None;
        } else {
            self.model_picker_submenu = submenu;
        }
        cx.notify();
    }

    /// Bring the current model's row into view whenever the picker shows the
    /// unfiltered list — on open, on a cleared query, and on tab switches.
    ///
    /// The request parks in the scroll handle until the row list next paints,
    /// so it may be issued from the open toggle before the deferred panel
    /// exists, and a tab whose models are still loading reveals the row once
    /// they arrive. Without a row to reveal it falls back to the top, so a
    /// scroll offset from an earlier open never leaks into a fresh list.
    pub(super) fn reveal_selected_picker_model(&self) {
        let session = self.selected_session();
        let provider = session
            .map(|session| session.provider)
            .unwrap_or(self.state.last_provider);
        let selected_model = session
            .and_then(|session| self.model_for_session(session))
            .or(self.state.last_model.as_deref())
            .or_else(|| {
                self.provider_probe(provider)
                    .and_then(ProviderProbe::preferred_model)
                    .map(|m| m.id.as_str())
            });
        let locked_provider: Option<ProviderKind> = None;
        let index = visible_picker_models(
            &self.probes,
            &self.state.recent_models,
            &self.state.favorite_models,
            &self.state.disabled_providers,
            locked_provider,
            self.model_picker_tab,
            "",
        )
        .iter()
        .position(|(kind, model)| *kind == provider && selected_model == Some(model.id.as_str()))
        .unwrap_or(0);
        self.model_picker_scroll.scroll_to_item(index);
    }

    /// Take the row the selection is on, defaulting to the first so `enter`
    /// works the moment the panel opens.
    fn choose_highlighted_model(
        &mut self,
        models: &[(ProviderKind, ProviderModel)],
        cx: &mut Context<Self>,
    ) {
        let Some((kind, model)) = models.get(self.model_picker_highlight.unwrap_or(0)) else {
            return;
        };
        let (kind, model_id) = (*kind, model.id.clone());
        self.choose_model(kind, model_id, cx);
    }

    fn set_pi_plan_mode(&mut self, target: PlanMode, cx: &mut Context<Self>) {
        let Some(session) = self.selected_session() else {
            return;
        };
        if !plan_mode_supported(session.provider) {
            return;
        }
        let provider = session.provider;
        let session_id = session.id;
        let current = self
            .plan_modes
            .get(&session_id)
            .copied()
            .unwrap_or_default();
        let commands = plan_mode_commands(provider, current, target);
        // Optimistic chip update: the driver applies the switch
        // asynchronously. Stash the pre-toggle mode so a
        // `PlanModeSwitchFailed` event can restore it.
        self.plan_mode_fallback.insert(session_id, current);
        self.plan_modes.insert(session_id, target);
        if let Some(runtime) = self.runtimes.get(&session_id) {
            runtime.driver.provider_control(commands);
        }
        cx.notify();
    }

    fn available_plan_modes(&self, provider: ProviderKind) -> Vec<PlanMode> {
        match provider {
            ProviderKind::Pi | ProviderKind::OhMyPi => {
                let has_plan = self
                    .slash_command_index
                    .iter()
                    .any(|command| command.name == "plan");
                let has_plannotator = self
                    .slash_command_index
                    .iter()
                    .any(|command| command.name == "plannotator-plan-mode");
                let mut modes = vec![PlanMode::Off];
                if has_plan {
                    modes.push(PlanMode::Plan);
                }
                if has_plannotator {
                    modes.push(PlanMode::Plannotator);
                }
                modes
            }
            ProviderKind::DeepSeek => {
                // Harness reports its command-backed `/plan` through the live
                // registry; without it there is nothing to toggle.
                if self
                    .slash_command_index
                    .iter()
                    .any(|command| command.name == "plan")
                {
                    vec![PlanMode::Off, PlanMode::Plan]
                } else {
                    vec![PlanMode::Off]
                }
            }
            ProviderKind::OpenCode
            | ProviderKind::OpenCode2
            | ProviderKind::Cursor
            | ProviderKind::Fx
            | ProviderKind::Claude
            | ProviderKind::Codex => {
                vec![PlanMode::Off, PlanMode::Plan]
            }
            _ => vec![PlanMode::Off],
        }
    }

    pub(super) fn render_pi_plan_mode_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.selected_session()?;
        if !plan_mode_supported(session.provider) {
            return None;
        }
        let available = self.available_plan_modes(session.provider);
        // A lone "No plan" entry carries no choice; hide the chip instead of
        // showing a one-item menu (e.g. DeepSeek before its registry lands).
        if available.len() < 2 {
            return None;
        }
        let selected = self
            .plan_modes
            .get(&session.id)
            .copied()
            .unwrap_or_default();
        let theme = Theme::current(cx);
        let weak = cx.entity().downgrade();
        let handle = self.menu_handle("pi-plan-mode", cx);
        Some(dropdown_menu(
            MenuChip::new("pi-plan-mode")
                .icon("icons/list.svg", theme.text_tertiary)
                .label(selected.label())
                .caret(false)
                .selected(handle.is_open()),
            "pi-plan-mode-menu",
            &handle,
            MenuAlign::AboveLeft,
            move |_| {
                let mut items = Vec::new();
                for mode in [PlanMode::Off, PlanMode::Plan, PlanMode::Plannotator] {
                    if !available.contains(&mode) {
                        continue;
                    }
                    let weak = weak.clone();
                    items.push(
                        MenuItem::new(mode.label(), move |_, cx| {
                            let _ = weak.update(cx, |this, cx| this.set_pi_plan_mode(mode, cx));
                        })
                        .selected(mode == selected),
                    );
                }
                items
            },
        ))
    }

    #[allow(dead_code)]
    pub(super) fn render_model_traits_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = Theme::current(cx);
        let session = self.selected_session();
        let provider = session
            .map(|session| session.provider)
            .unwrap_or(self.state.last_provider);
        let model_name = session
            .and_then(|session| self.model_for_session(session))
            .or(self.state.last_model.as_deref())
            .or_else(|| {
                self.provider_probe(provider)
                    .and_then(ProviderProbe::preferred_model)
                    .map(|m| m.id.as_str())
            })?;
        let model = self
            .provider_probe(provider)?
            .models
            .iter()
            .find(|candidate| candidate.id == model_name)?;
        if model.reasoning_efforts.is_empty()
            && model.service_tiers.is_empty()
            && model.context_windows.is_empty()
        {
            return None;
        }

        let selected_effort = session
            .and_then(|s| s.reasoning_effort.as_deref())
            .or(self.state.last_reasoning_effort.as_deref())
            .filter(|selected| {
                model
                    .reasoning_efforts
                    .iter()
                    .any(|option| option.id == *selected)
            })
            .or(model.default_reasoning_effort.as_deref())
            .or_else(|| {
                model
                    .reasoning_efforts
                    .first()
                    .map(|option| option.id.as_str())
            })
            .map(str::to_owned);
        let effort_label = selected_effort.as_deref().and_then(|selected| {
            model
                .reasoning_efforts
                .iter()
                .find(|option| option.id == selected)
                .map(|option| option.label.clone())
        });

        let selected_tier = session
            .and_then(|s| s.service_tier.as_deref())
            .or(self.state.last_service_tier.as_deref())
            .filter(|selected| {
                *selected == "default"
                    || model
                        .service_tiers
                        .iter()
                        .any(|option| option.id == *selected)
            })
            .or(model.default_service_tier.as_deref())
            .unwrap_or("default")
            .to_owned();
        let tier_label = if selected_tier == "default" {
            tr!("models.standard")
        } else {
            model
                .service_tiers
                .iter()
                .find(|option| option.id == selected_tier)
                .map(|option| option.label.clone())
                .unwrap_or_else(|| selected_tier.clone())
        };
        let selected_window = session
            .and_then(|s| s.context_window.as_deref())
            .or(self.state.last_context_window.as_deref())
            .filter(|selected| {
                model
                    .context_windows
                    .iter()
                    .any(|option| option.id == *selected)
            })
            .or(model.default_context_window.as_deref())
            .or_else(|| {
                model
                    .context_windows
                    .first()
                    .map(|option| option.id.as_str())
            })
            .map(str::to_owned);
        // A non-default window changes what the session costs and how much it
        // can hold, so it reads on the chip rather than only inside the menu.
        let window_label = selected_window
            .as_deref()
            .filter(|selected| model.default_context_window.as_deref() != Some(selected))
            .and_then(|selected| {
                model
                    .context_windows
                    .iter()
                    .find(|option| option.id == selected)
                    .map(|option| option.label.clone())
            });

        let fast = selected_tier == "fast" || tier_label.eq_ignore_ascii_case("fast");
        let trigger_label = match (
            effort_label.unwrap_or_else(|| tier_label.clone()),
            window_label,
        ) {
            (label, Some(window)) => format!("{label} · {window}"),
            (label, None) => label,
        };
        let reasoning_efforts = model.reasoning_efforts.clone();
        let default_effort = model.default_reasoning_effort.clone();
        let service_tiers = model.service_tiers.clone();
        let context_windows = model.context_windows.clone();
        let default_window = model.default_context_window.clone();
        let default_tier = model
            .default_service_tier
            .clone()
            .unwrap_or_else(|| "default".to_owned());
        let weak = cx.entity().downgrade();
        let handle = self.menu_handle("model-traits", cx);
        Some(dropdown_menu(
            MenuChip::new("model-traits")
                .when(fast, |trigger| {
                    trigger.icon("icons/zap.svg", theme.text_secondary)
                })
                .label(trigger_label)
                .caret(false)
                .selected(handle.is_open()),
            "model-traits-menu",
            &handle,
            MenuAlign::AboveLeft,
            move |_| {
                let mut items = Vec::new();
                if !reasoning_efforts.is_empty() {
                    items.push(MenuItem::Header(tr!("models.reasoning").into()));
                    for option in reasoning_efforts.clone() {
                        let weak = weak.clone();
                        let effort = option.id;
                        let is_default = default_effort.as_deref() == Some(effort.as_str());
                        let selected = selected_effort.as_deref() == Some(effort.as_str());
                        items.push(
                            traits_choice(theme, option.label, is_default, selected).on_click(
                                move |_, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.set_reasoning_effort(effort.clone(), cx);
                                    });
                                },
                            ),
                        );
                    }
                }
                if !service_tiers.is_empty() {
                    if !reasoning_efforts.is_empty() {
                        items.push(MenuItem::Separator);
                    }
                    items.push(MenuItem::Header(tr!("models.service_tier").into()));
                    let weak_standard = weak.clone();
                    items.push(
                        traits_choice(
                            theme,
                            tr!("models.standard"),
                            default_tier == "default",
                            selected_tier == "default",
                        )
                        .on_click(move |_, cx| {
                            let _ = weak_standard.update(cx, |this, cx| {
                                this.set_service_tier("default".to_owned(), cx);
                            });
                        }),
                    );
                    for option in service_tiers.clone() {
                        let weak = weak.clone();
                        let tier = option.id;
                        let is_default = default_tier == tier;
                        let selected = selected_tier == tier;
                        items.push(
                            traits_choice(theme, option.label, is_default, selected).on_click(
                                move |_, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.set_service_tier(tier.clone(), cx);
                                    });
                                },
                            ),
                        );
                    }
                }
                if !context_windows.is_empty() {
                    if !reasoning_efforts.is_empty() || !service_tiers.is_empty() {
                        items.push(MenuItem::Separator);
                    }
                    items.push(MenuItem::Header(tr!("models.context_window").into()));
                    for option in context_windows.clone() {
                        let weak = weak.clone();
                        let window = option.id;
                        let is_default = default_window.as_deref() == Some(window.as_str());
                        let selected = selected_window.as_deref() == Some(window.as_str());
                        items.push(
                            traits_choice(theme, option.label, is_default, selected).on_click(
                                move |_, cx| {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.set_context_window(window.clone(), cx);
                                    });
                                },
                            ),
                        );
                    }
                }
                items
            },
        ))
    }

    #[allow(dead_code)]
    pub(super) fn render_access_control(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = Theme::current(cx);
        let selected_mode = self
            .selected_session()
            .map(|session| session.runtime_mode)
            .unwrap_or(self.state.last_runtime_mode);
        let weak = cx.entity().downgrade();
        let handle = self.menu_handle("runtime-mode", cx);
        dropdown_menu(
            MenuChip::new("runtime-mode")
                .icon(selected_mode.icon(), theme.text_tertiary)
                .label(selected_mode.label())
                .caret(false)
                .selected(handle.is_open()),
            "runtime-mode-menu",
            &handle,
            MenuAlign::AboveLeft,
            move |_| {
                RuntimeMode::ACCESS_OPTIONS
                    .into_iter()
                    .map(|option| {
                        let weak = weak.clone();
                        let selected = option == selected_mode;
                        MenuItem::custom(move |_, _| {
                            div()
                                .w(px(288.0))
                                .py(px(4.0))
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(icon(option.icon(), 14.0, theme.text_tertiary))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .w_full()
                                                .truncate()
                                                .text_size(sp(12.5))
                                                .font_weight(if selected {
                                                    FontWeight::SEMIBOLD
                                                } else {
                                                    FontWeight::MEDIUM
                                                })
                                                .text_color(theme.text)
                                                .child(option.label()),
                                        )
                                        .child(
                                            div()
                                                .w_full()
                                                .mt(px(2.0))
                                                .text_size(sp(12.5))
                                                .line_height(sp(14.0))
                                                .whitespace_normal()
                                                .text_color(theme.text_tertiary)
                                                .child(option.description()),
                                        ),
                                )
                                .when(selected, |element| {
                                    element.child(icon(
                                        "icons/check.svg",
                                        11.0,
                                        theme.text_tertiary,
                                    ))
                                })
                                .into_any_element()
                        })
                        .on_click(move |_, cx| {
                            let _ = weak.update(cx, |this, cx| this.set_runtime_mode(option, cx));
                        })
                    })
                    .collect()
            },
        )
    }

    #[allow(dead_code)]
    pub(super) fn render_agent_preset_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let provider = self
            .selected_session()
            .map(|s| s.provider)
            .unwrap_or(self.state.last_provider);
        if provider != ProviderKind::DeepSeek {
            return None;
        }
        if self
            .selected_session()
            .is_some_and(|session| session.has_started() || session.is_busy())
        {
            return None;
        }
        let presets = self
            .provider_probe(ProviderKind::DeepSeek)
            .map(|probe| probe.agent_presets.clone())
            .unwrap_or_default();
        if presets.is_empty() {
            return None;
        }
        let selected_id = self
            .selected_session()
            .and_then(|session| self.agent_preset_for_session(session))
            .or_else(|| {
                self.provider_probe(ProviderKind::DeepSeek)
                    .and_then(ProviderProbe::preferred_agent_preset)
                    .map(|preset| preset.id.clone())
            })?;
        let selected_label = self
            .provider_probe(ProviderKind::DeepSeek)
            .and_then(|probe| {
                probe
                    .agent_presets
                    .iter()
                    .find(|preset| preset.id == selected_id)
            })
            .map(|preset| preset.display_name())
            .unwrap_or_else(|| selected_id.clone());
        let theme = Theme::current(cx);
        let weak = cx.entity().downgrade();
        let refresh_weak = weak.clone();
        let handle = self.menu_handle_with("agent-preset", cx, move |open, _, cx| {
            if open {
                let _ = refresh_weak.update(cx, |this, _| {
                    this.refresh_provider_model_discovery(ProviderKind::DeepSeek);
                });
            }
        });
        let trigger = MenuChip::new("agent-preset")
            .icon("icons/bot.svg", theme.text_tertiary)
            .label(selected_label)
            .caret(false)
            .selected(handle.is_open());

        Some(dropdown_menu(
            trigger,
            "agent-preset-menu",
            &handle,
            MenuAlign::AboveLeft,
            move |_| {
                presets
                    .clone()
                    .into_iter()
                    .map(|preset| {
                        let weak = weak.clone();
                        let preset_id = preset.id.clone();
                        let selected = preset_id == selected_id;
                        let name = if preset.is_custom {
                            format!("{} · {}", preset.display_name(), tr!("agent_preset.custom"))
                        } else {
                            preset.display_name()
                        };
                        let description = preset
                            .display_description()
                            .unwrap_or_else(|| tr!("agent_preset.no_description"))
                            // GPUI wraps at Unicode line-break opportunities,
                            // but an underscored tool name is otherwise one
                            // indivisible word. The zero-width spaces preserve
                            // its visible spelling while allowing the menu to
                            // keep it inside the card.
                            .replace('_', "_\u{200b}");
                        MenuItem::custom(move |_, _| {
                            div()
                                .w(px(340.0))
                                .py(px(5.0))
                                .overflow_hidden()
                                .flex()
                                .items_center()
                                .gap(px(10.0))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .child(
                                            div()
                                                .w_full()
                                                .truncate()
                                                .text_size(sp(12.5))
                                                .font_weight(if selected {
                                                    FontWeight::SEMIBOLD
                                                } else {
                                                    FontWeight::MEDIUM
                                                })
                                                .text_color(theme.text)
                                                .child(name.clone()),
                                        )
                                        .child(
                                            div()
                                                .w_full()
                                                .mt(px(2.0))
                                                .text_size(sp(12.5))
                                                .line_height(sp(14.0))
                                                .whitespace_normal()
                                                .overflow_hidden()
                                                .text_color(theme.text_tertiary)
                                                .child(description.clone()),
                                        ),
                                )
                                .when(selected, |element| {
                                    element.child(icon(
                                        "icons/check.svg",
                                        11.0,
                                        theme.text_tertiary,
                                    ))
                                })
                                .into_any_element()
                        })
                        .on_click(move |_, cx| {
                            let _ = weak.update(cx, |this, cx| {
                                this.set_agent_preset(preset_id.clone(), cx);
                            });
                        })
                    })
                    .collect()
            },
        ))
    }

    /// The thread-goal chip: present only while the provider reports a goal,
    /// it pairs a target icon with the status phrase (and budget consumption)
    /// and opens the goal dialog. `/goal` is the keyboard route to the same
    /// surface.
    pub(super) fn render_goal_control(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let session = self.selected_session()?;
        let goal = session.thread_goal.as_ref()?;
        let session_id = session.id;
        let theme = Theme::current(cx);
        let color = super::goal_dialog::goal_status_color(goal.status, &theme);
        // Elapsed pursuit time accrues only while a turn actually runs,
        // matching how the provider accounts it.
        let live_elapsed_seconds = (goal.status == crate::model::ThreadGoalStatus::Active
            && session.is_busy())
        .then(|| self.goal_observed_at.get(&session_id))
        .flatten()
        .map_or(0, |observed| observed.elapsed().as_secs() as i64);
        let label = super::goal_dialog::goal_chip_label(goal, live_elapsed_seconds);
        let objective = goal.objective.clone();
        let weak = cx.entity().downgrade();
        Some(
            div()
                .id("composer-goal")
                .h(px(24.0))
                .px(px(7.0))
                .rounded(px(6.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .cursor_default()
                .text_size(sp(12.5))
                .line_height(sp(14.0))
                .text_color(color)
                .child(icon("icons/target.svg", 10.5, color))
                .child(div().max_w(px(220.0)).truncate().child(label))
                .hover(|element| element.bg(theme.overlay))
                .tooltip(Tooltip::text(objective))
                .on_click(move |_, _, cx| {
                    let _ = weak.update(cx, |this, cx| {
                        this.request_goal_dialog(session_id, None, false, cx);
                    });
                })
                .into_any_element(),
        )
    }

    /// Stage files dropped onto the composer as attachment chips. The mention
    /// each chip will submit takes the autocomplete's form: relative to the
    /// project root when the file is inside it, absolute otherwise,
    /// directories with a trailing slash.
    pub(super) fn stage_dropped_files(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.stage_attachment_paths(paths.paths(), cx) {
            return;
        }
        let focus = self.composer.read(cx).focus();
        window.focus(&focus, cx);
    }

    fn stage_attachment_paths(&mut self, paths: &[PathBuf], cx: &mut Context<Self>) -> bool {
        if paths.is_empty() {
            return false;
        }
        let paths = paths.to_vec();
        let daemon = self.daemon.clone();
        let draft_owner = self.selected_composer_draft_key();
        cx.spawn(async move |insulator, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    let mut stored = Vec::with_capacity(paths.len());
                    for source_path in paths {
                        let (name, upload, image_bytes) =
                            attachment_upload_from_path(&source_path)?;
                        let is_image = image_bytes.is_some();
                        let preview_image = image_bytes.and_then(|bytes| {
                            image_preview::image_format_for_name(&name)
                                .map(|format| Arc::new(gpui::Image::from_bytes(format, bytes)))
                        });
                        let response = daemon.client().request(
                            Uuid::nil(),
                            Uuid::nil(),
                            insulator_client::Command::ImportAttachment { name, upload },
                        )?;
                        let insulator_client::ResponsePayload::AttachmentStored { attachment } =
                            response
                        else {
                            anyhow::bail!("the daemon returned an invalid attachment response");
                        };
                        stored.push((attachment, preview_image, is_image));
                    }
                    Ok::<_, anyhow::Error>(stored)
                })
                .await;
            let _ = insulator.update(cx, |insulator, cx| match result {
                Ok(stored) => {
                    if insulator.selected_composer_draft_key() != draft_owner {
                        return;
                    }
                    let mut changed = false;
                    for (attachment, preview_image, is_image) in stored {
                        changed |= insulator.stage_daemon_attachment(
                            attachment.path,
                            attachment.name,
                            attachment.is_dir,
                            is_image,
                            attachment.reference,
                            preview_image,
                        );
                    }
                    if changed {
                        insulator.schedule_composer_draft_save(cx);
                        cx.notify();
                    }
                }
                Err(error) => {
                    insulator.show_toast(error.to_string());
                    cx.notify();
                }
            });
        })
        .detach();
        true
    }

    fn stage_daemon_attachment(
        &mut self,
        path: PathBuf,
        name: String,
        is_dir: bool,
        is_image: bool,
        reference: String,
        client_preview_image: Option<Arc<gpui::Image>>,
    ) -> bool {
        if self.composer_attachments.iter().any(|attachment| {
            attachment.path == path
                || attachment.blob_reference.as_deref() == Some(reference.as_str())
        }) {
            return false;
        }
        let mut mention = path.display().to_string();
        if is_dir && !mention.ends_with('/') {
            mention.push('/');
        }
        self.composer_attachments.push(ComposerAttachment {
            path,
            client_preview_image,
            mention,
            name: SharedString::from(name),
            is_dir,
            is_image,
            blob_reference: Some(reference),
        });
        true
    }

    /// Stage the clipboard's primary image/file representation. On-disk paths
    /// reuse drop handling immediately; raw image bytes are copied into Insulator's
    /// durable blob store on the background executor before their chip appears.
    pub(super) fn stage_pasted_attachments(
        &mut self,
        entries: Vec<ClipboardEntry>,
        cx: &mut Context<Self>,
    ) {
        let mut paths = Vec::new();
        let mut images = Vec::new();
        for entry in entries {
            match entry {
                ClipboardEntry::Image(image) if !image.bytes.is_empty() => images.push(image),
                ClipboardEntry::ExternalPaths(external) => {
                    paths.extend(external.paths().iter().cloned())
                }
                ClipboardEntry::String(_) | ClipboardEntry::Image(_) => {}
            }
        }
        self.stage_attachment_paths(&paths, cx);
        if images.is_empty() {
            return;
        }

        let daemon = self.daemon.clone();
        let draft_owner = self.selected_composer_draft_key();
        cx.spawn(async move |insulator, cx| {
            let stored = cx
                .background_executor()
                .spawn(async move {
                    let image_count = images.len();
                    images
                        .into_iter()
                        .enumerate()
                        .map(|(index, image)| {
                            let preview_image = Arc::new(image);
                            let bytes = preview_image.bytes.clone();
                            let response = daemon
                                .client()
                                .request(
                                    Uuid::nil(),
                                    Uuid::nil(),
                                    insulator_client::Command::StoreBlob {
                                        mime_type: preview_image.format.mime_type().to_owned(),
                                        bytes,
                                    },
                                )
                                .map_err(|error| error.to_string())?;
                            let insulator_client::ResponsePayload::BlobStored { reference, path } =
                                response
                            else {
                                return Err("the daemon returned an invalid blob response".into());
                            };
                            let extension = path
                                .extension()
                                .and_then(|extension| extension.to_str())
                                .unwrap_or("png");
                            let name = if image_count == 1 {
                                format!("image.{extension}")
                            } else {
                                format!("image-{}.{extension}", index + 1)
                            };
                            Ok::<_, String>((path, name, reference, preview_image))
                        })
                        .collect::<Result<Vec<_>, _>>()
                })
                .await;
            let _ = insulator.update(cx, |insulator, cx| match stored {
                Ok(stored) => {
                    if insulator.selected_composer_draft_key() != draft_owner {
                        return;
                    }
                    let mut staged = false;
                    for (path, name, reference, preview_image) in stored {
                        staged |= insulator.stage_daemon_attachment(
                            path,
                            name,
                            false,
                            true,
                            reference,
                            Some(preview_image),
                        );
                    }
                    if staged {
                        insulator.schedule_composer_draft_save(cx);
                        cx.notify();
                    }
                }
                Err(error) => {
                    insulator.show_toast(tr!("errors.store_pasted_image", error = error));
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// The text and attachment presentation accepted from the composer. The
    /// stored prompt keeps its `@` mentions and visible command syntax, while
    /// sent-message UI uses `display_content` and retained attachment metadata.
    pub(super) fn submission_with_attachments(
        &mut self,
        prompt: &str,
        cx: &mut Context<Self>,
    ) -> Option<ComposerSubmission> {
        if self.execute_local_composer_command(prompt, cx) {
            return None;
        }
        // Nothing installed or switched on can run this. Refuse before the
        // draft is consumed, so the text and its attachments survive until a
        // provider is available — every send route lands here, so `enter`,
        // the button, and steering are all covered by this one check.
        if self.model_picker_has_no_providers() {
            return None;
        }
        for attachment in &self.composer_attachments {
            if let (Some(reference), Some(image)) = (
                attachment.blob_reference.as_ref(),
                attachment.client_preview_image.as_ref(),
            ) {
                self.remote_images
                    .borrow_mut()
                    .insert(reference.clone(), Some(image.clone()));
            }
        }
        let attachments = self
            .composer_attachments
            .drain(..)
            .map(MessageAttachment::from)
            .collect::<Vec<_>>();
        let mentions = attachments
            .iter()
            .map(|attachment| attachment.mention.clone())
            .collect::<Vec<_>>();
        let prompt_with_file_context = self.file_editor_selection_prompt(prompt);
        let submission = merged_submission(&prompt_with_file_context, &mentions)?;
        self.clear_file_editor_selection(Some(cx));
        let display_content = (!attachments.is_empty()).then(|| prompt.trim().to_owned());
        self.discard_current_composer_draft(cx);
        Some(ComposerSubmission {
            prompt: submission,
            display_content,
            attachments,
        })
    }

    pub(super) fn execute_local_composer_command(
        &mut self,
        prompt: &str,
        cx: &mut Context<Self>,
    ) -> bool {
        self.execute_resume_composer_command(prompt, cx)
            || self.execute_fast_mode_toggle(prompt, cx)
            || self.execute_pi_plan_command(prompt, cx)
            || self.execute_goal_composer_command(prompt, cx)
    }

    fn execute_resume_composer_command(&mut self, prompt: &str, cx: &mut Context<Self>) -> bool {
        if !crate::composer_complete::is_resume_submission(prompt) {
            return false;
        }
        self.composer.update(cx, |input, cx| input.clear(cx));
        // Submission notifications already hold this entity mutably. Dispatch
        // after that effect returns so the window action can safely re-enter
        // Insulator and move focus into the Resume picker.
        cx.defer(|cx| cx.dispatch_action(&OpenResumePicker));
        true
    }

    fn execute_pi_plan_command(&mut self, prompt: &str, cx: &mut Context<Self>) -> bool {
        let target = match prompt.trim() {
            "/plan"
                if self
                    .slash_command_index
                    .iter()
                    .any(|command| command.name == "plan") =>
            {
                PlanMode::Plan
            }
            "/plannotator-plan-mode"
                if self
                    .slash_command_index
                    .iter()
                    .any(|command| command.name == "plannotator-plan-mode") =>
            {
                PlanMode::Plannotator
            }
            _ => return false,
        };
        let Some(session) = self.selected_session() else {
            return false;
        };
        // Only command-backed toggle providers consume the typed command.
        // Everywhere else the text stays a normal submission so a
        // user-defined `/plan` (or the CLI's own interpretation of it)
        // still reaches the provider: native-mode providers switch through
        // the chip, never through intercepted text.
        if !matches!(
            session.provider,
            ProviderKind::Pi | ProviderKind::OhMyPi | ProviderKind::DeepSeek
        ) {
            return false;
        }
        if !plan_mode_supported(session.provider) {
            return false;
        }
        if !self
            .available_plan_modes(session.provider)
            .contains(&target)
        {
            return false;
        }
        let current = self
            .plan_modes
            .get(&session.id)
            .copied()
            .unwrap_or_default();
        self.set_pi_plan_mode(
            if current == target {
                PlanMode::Off
            } else {
                target
            },
            cx,
        );
        self.composer.update(cx, |input, cx| input.clear(cx));
        true
    }

    /// Bridge Codex's native `/goal` command without starting a turn. Reads run
    /// against the session's cached goal; mutations go to the app-server and
    /// echo back as `GoalUpdated` events.
    fn execute_goal_composer_command(&mut self, prompt: &str, cx: &mut Context<Self>) -> bool {
        use crate::composer_complete::GoalCommand;
        use crate::model::{GoalOperation, ThreadGoalStatus};
        let Some((session_id, command, current_goal)) =
            self.selected_session().and_then(|session| {
                let command = crate::composer_complete::parse_goal_submission(
                    session.provider,
                    prompt,
                    &self.slash_command_index,
                )?;
                Some((session.id, command, session.thread_goal.clone()))
            })
        else {
            return false;
        };
        match command {
            GoalCommand::Show | GoalCommand::Edit => {
                self.request_goal_dialog(session_id, None, false, cx);
            }
            GoalCommand::Pause => {
                self.dispatch_goal_operation(
                    session_id,
                    GoalOperation::Set {
                        objective: None,
                        status: Some(ThreadGoalStatus::Paused),
                        replace: false,
                    },
                    cx,
                );
            }
            GoalCommand::Resume => {
                self.dispatch_goal_operation(
                    session_id,
                    GoalOperation::Set {
                        objective: None,
                        status: Some(ThreadGoalStatus::Active),
                        replace: false,
                    },
                    cx,
                );
            }
            GoalCommand::Clear => {
                self.dispatch_goal_operation(session_id, GoalOperation::Clear, cx);
            }
            GoalCommand::Set(objective) => match &current_goal {
                // Replacing unfinished work needs a look at what it replaces;
                // the dialog carries the confirmation.
                Some(goal) if !goal.status.is_terminal() => {
                    self.request_goal_dialog(session_id, Some(objective), true, cx);
                }
                Some(_) | None => {
                    self.dispatch_goal_operation(
                        session_id,
                        GoalOperation::Set {
                            objective: Some(objective),
                            status: Some(ThreadGoalStatus::Active),
                            replace: current_goal.is_some(),
                        },
                        cx,
                    );
                }
            },
        }
        self.composer.update(cx, |input, cx| input.clear(cx));
        cx.notify();
        true
    }

    fn execute_fast_mode_toggle(&mut self, prompt: &str, cx: &mut Context<Self>) -> bool {
        let provider = self
            .selected_session()
            .map(|s| s.provider)
            .unwrap_or(self.state.last_provider);
        if !crate::composer_complete::is_fast_mode_toggle_submission(
            provider,
            prompt,
            &self.slash_command_index,
        ) {
            return false;
        }
        let model_name = self
            .selected_session()
            .and_then(|s| self.model_for_session(s))
            .or(self.state.last_model.as_deref())
            .or_else(|| {
                self.provider_probe(provider)
                    .and_then(ProviderProbe::preferred_model)
                    .map(|m| m.id.as_str())
            });
        let Some(model) = model_name.and_then(|name| {
            self.provider_probe(provider)?
                .models
                .iter()
                .find(|candidate| candidate.id == name)
        }) else {
            return false;
        };
        let current_tier = self
            .selected_session()
            .and_then(|s| s.service_tier.as_deref())
            .or(self.state.last_service_tier.as_deref());
        let Some(next_tier) =
            crate::composer_complete::toggled_fast_service_tier(current_tier, &model.service_tiers)
        else {
            return false;
        };
        let enabled = next_tier != "default";
        // Clearing emits an Edited event. Apply the tier afterward so any
        // draft refresh caused by that event cannot repaint the old choice.
        self.composer.update(cx, |input, cx| input.clear(cx));
        self.set_service_tier(next_tier, cx);
        self.show_success_toast(tr!(if enabled {
            "commands.fast_enabled"
        } else {
            "commands.fast_disabled"
        }));
        true
    }

    pub(super) fn restore_composer_submission(
        &mut self,
        submission: ComposerSubmission,
        cx: &mut Context<Self>,
    ) {
        self.composer_attachments = submission
            .attachments
            .into_iter()
            .map(ComposerAttachment::from)
            .collect();
        let content = submission.display_content.unwrap_or(submission.prompt);
        self.composer
            .update(cx, |input, cx| input.set_content(content, cx));
        self.schedule_composer_draft_save(cx);
        cx.notify();
    }

    /// The staged-attachment chips above the input: a thumbnail tile per
    /// image, a file-type icon and basename for everything else, each with a
    /// floating remove button — T3 Code's attachment row in graphite.
    fn render_composer_attachments(&self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::current(cx);
        let mut row = div()
            .px(px(14.0))
            .pt(px(2.0))
            .pb(px(8.0))
            .flex()
            .flex_wrap()
            .gap(px(8.0));
        for (index, attachment) in self.composer_attachments.iter().enumerate() {
            let menu = self.menu_handle(format!("composer-attachment-{index}-menu"), cx);
            let icon_path = if attachment.is_dir {
                "icons/folder.svg"
            } else {
                super::right_panel::file_icon_for_path(&attachment.mention)
            };
            let mut tile = div()
                .id(SharedString::from(format!("composer-attachment-{index}")))
                .relative()
                .w(px(64.0))
                .h(px(64.0))
                .rounded(px(8.0))
                .overflow_hidden()
                .border_1()
                .border_color(theme.border)
                .bg(theme.inset)
                .track_focus(menu.trigger_focus_handle())
                .tab_index(0)
                .focus_visible(|style| style.border_color(theme.accent))
                .tooltip(Tooltip::text(format!("@{}", attachment.mention)));
            let attachment_image = attachment.client_preview_image.clone().or_else(|| {
                attachment
                    .is_image
                    .then(|| {
                        attachment.blob_reference.as_deref().and_then(|reference| {
                            self.image_for_reference(
                                reference,
                                Some(&attachment.path),
                                Some(attachment.name.as_ref()),
                                cx,
                            )
                        })
                    })
                    .flatten()
            });
            let can_reveal = !self.daemon.is_remote();
            if attachment.is_image {
                if let Some(attachment_image) = attachment_image.as_ref() {
                    let preview_image = attachment_image.clone();
                    let preview_name = attachment.name.clone();
                    tile = tile.child(
                        div()
                            .id(SharedString::from(format!(
                                "composer-attachment-{index}-preview"
                            )))
                            .size_full()
                            .cursor_default()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_image_preview(
                                    preview_image.clone(),
                                    preview_name.clone(),
                                    window,
                                    cx,
                                );
                                cx.stop_propagation();
                            }))
                            .child(
                                img(attachment_image.clone())
                                    .size_full()
                                    .object_fit(ObjectFit::Cover),
                            ),
                    );
                } else {
                    tile = tile.child(
                        div()
                            .size_full()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("icons/file-types/image.svg", 16.0, theme.text_ghost)),
                    );
                }
            } else {
                tile = tile.child(
                    div()
                        .size_full()
                        .px(px(5.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(5.0))
                        .child(icon(icon_path, 16.0, theme.text_tertiary))
                        .child(
                            div().w_full().flex().justify_center().child(
                                div()
                                    .max_w_full()
                                    .truncate()
                                    .text_size(sp(12.5))
                                    .text_color(theme.text_tertiary)
                                    .child(attachment.name.clone()),
                            ),
                        ),
                );
            }
            let key_menu = menu.clone();
            let key_image = attachment_image.clone();
            let key_name = attachment.name.clone();
            let is_image = attachment.is_image;
            tile = tile.on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                let key = event.keystroke.key.as_str();
                if is_image
                    && matches!(key, "enter" | "space")
                    && let Some(key_image) = key_image.as_ref()
                {
                    this.open_image_preview(key_image.clone(), key_name.clone(), window, cx);
                    cx.stop_propagation();
                } else if key == "f10" && event.keystroke.modifiers.shift {
                    key_menu.open_context_menu(window, cx);
                    cx.stop_propagation();
                }
            }));
            let tile = tile.child(
                div()
                    .id(SharedString::from(format!(
                        "composer-attachment-remove-{index}"
                    )))
                    .absolute()
                    .top(px(3.0))
                    .right(px(3.0))
                    .w(px(16.0))
                    .h(px(16.0))
                    .tab_index(0)
                    .rounded(px(5.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_default()
                    .bg(theme.canvas.opacity(0.8))
                    .focus_visible(|style| style.border_1().border_color(theme.accent))
                    .hover(|element| element.bg(theme.canvas.opacity(0.95)))
                    .active(|element| element.opacity(0.8))
                    .child(icon("icons/x.svg", 9.0, theme.text_secondary))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        if index < this.composer_attachments.len() {
                            this.composer_attachments.remove(index);
                            this.schedule_composer_draft_save(cx);
                            cx.notify();
                        }
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            if index < this.composer_attachments.len() {
                                this.composer_attachments.remove(index);
                                this.schedule_composer_draft_save(cx);
                                cx.notify();
                            }
                            cx.stop_propagation();
                        }
                    })),
            );
            let reveal_path = attachment.path.clone();
            row = row.child(context_menu(
                tile,
                SharedString::from(format!("composer-attachment-{index}-context-menu")),
                &menu,
                move |_| image_preview::attachment_menu_items(reveal_path.clone(), can_reveal),
            ));
        }
        row
    }

    /// The pending follow-up queue between the transcript and the composer: a
    /// single card tucked against the composer's top edge, one row per queued
    /// message. A row pulls its text back into the composer on click and
    /// carries steer/remove/more controls on the right.
    pub(super) fn render_queued_messages(&self, cx: &mut Context<Self>) -> Option<Div> {
        let session_id = self.state.selected_session?;
        let session = self.selected_session()?;
        if session.queued_messages.is_empty() {
            return None;
        }
        let theme = Theme::current(cx);
        let steerable = self.session_can_steer(session);
        let mut list = div().flex().flex_col().py(px(4.0));
        for message in &session.queued_messages {
            let message_id = message.id;
            let content = if message.visible_content().trim().is_empty() {
                message
                    .attachments
                    .iter()
                    .map(|attachment| attachment.name.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                message.visible_content().to_owned()
            };
            let steer_control = steerable.then(|| {
                div()
                    .id(SharedString::from(format!(
                        "queued-message-steer-{message_id}"
                    )))
                    .h(px(24.0))
                    .px(px(7.0))
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .cursor_default()
                    .tab_index(0)
                    .focus_visible(|style| style.border_1().border_color(theme.accent))
                    .hover(|element| element.bg(theme.overlay_strong))
                    .active(|element| element.opacity(0.8))
                    .text_size(sp(12.5))
                    .text_color(theme.text_secondary)
                    .child(icon(
                        "icons/corner-down-right.svg",
                        11.0,
                        theme.text_secondary,
                    ))
                    .child(tr!("composer.steer"))
                    .tooltip(Tooltip::text(tr!("composer.steer_current")))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.steer_queued_message(session_id, message_id, cx);
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.steer_queued_message(session_id, message_id, cx);
                            cx.stop_propagation();
                        }
                    }))
            });
            let menu_handle = self.menu_handle(format!("queued-message-menu-{message_id}"), cx);
            let menu_open = menu_handle.is_open();
            let weak = cx.entity().downgrade();
            let more_control = dropdown_menu(
                div()
                    .id(SharedString::from(format!(
                        "queued-message-more-{message_id}"
                    )))
                    .w(px(24.0))
                    .h(px(24.0))
                    .rounded(px(6.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_default()
                    .focus_visible(|style| style.border_1().border_color(theme.accent))
                    .when(menu_open, |element| element.bg(theme.overlay_strong))
                    .hover(|element| element.bg(theme.overlay_strong))
                    .active(|element| element.opacity(0.8))
                    .child(icon("icons/ellipsis.svg", 12.5, theme.text_secondary)),
                SharedString::from(format!("queued-message-more-menu-{message_id}")),
                &menu_handle,
                MenuAlign::BelowRight,
                move |_| {
                    let edit_weak = weak.clone();
                    let remove_weak = weak.clone();
                    vec![
                        MenuItem::new(tr!("composer.edit_in_composer"), move |window, cx| {
                            let _ = edit_weak.update(cx, |this, cx| {
                                this.edit_queued_message(session_id, message_id, window, cx);
                            });
                        })
                        .icon("icons/pencil.svg"),
                        MenuItem::new(tr!("composer.remove_followup"), move |_, cx| {
                            let _ = remove_weak.update(cx, |this, cx| {
                                this.remove_queued_message(session_id, message_id, cx);
                            });
                        })
                        .icon("icons/trash.svg"),
                    ]
                },
            );
            list = list.child(
                div()
                    .id(SharedString::from(format!("queued-message-{message_id}")))
                    .h(px(30.0))
                    .pl(px(12.0))
                    .pr(px(6.0))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .cursor_default()
                    .tab_index(0)
                    .focus_visible(|style| style.border_1().border_color(theme.accent))
                    .hover(|element| element.bg(theme.overlay))
                    .tooltip(Tooltip::text(tr!("composer.edit_in_composer")))
                    .child(icon("icons/queue.svg", 12.0, theme.text_tertiary))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(sp(12.5))
                            .text_color(theme.text)
                            .child(SharedString::from(content)),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .children(steer_control)
                            .child(
                                div()
                                    .id(SharedString::from(format!(
                                        "queued-message-remove-{message_id}"
                                    )))
                                    .w(px(24.0))
                                    .h(px(24.0))
                                    .rounded(px(6.0))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_default()
                                    .tab_index(0)
                                    .focus_visible(|style| {
                                        style.border_1().border_color(theme.accent)
                                    })
                                    .hover(|element| element.bg(theme.overlay_strong))
                                    .active(|element| element.opacity(0.8))
                                    .child(icon("icons/trash.svg", 12.0, theme.text_secondary))
                                    .tooltip(Tooltip::text(tr!("composer.remove_followup")))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.stop_propagation();
                                        this.remove_queued_message(session_id, message_id, cx);
                                    }))
                                    .on_key_down(cx.listener(
                                        move |this, event: &KeyDownEvent, _, cx| {
                                            if matches!(
                                                event.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) {
                                                this.remove_queued_message(
                                                    session_id, message_id, cx,
                                                );
                                                cx.stop_propagation();
                                            }
                                        },
                                    )),
                            )
                            .child(more_control),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.edit_queued_message(session_id, message_id, window, cx);
                    }))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            this.edit_queued_message(session_id, message_id, window, cx);
                            cx.stop_propagation();
                        }
                    })),
            );
        }
        Some(
            div().flex_none().px(px(20.0)).child(
                div()
                    .w_full()
                    .max_w(px(CONTENT_MAX_WIDTH))
                    .mx_auto()
                    .px(px(14.0))
                    .child(
                        div()
                            .rounded_tl(px(12.0))
                            .rounded_tr(px(12.0))
                            .border_t_1()
                            .border_l_1()
                            .border_r_1()
                            .border_color(theme.border)
                            .bg(theme.composer)
                            // Row hover fills are full-width rectangles; clip
                            // them to the card's rounded corners.
                            .overflow_hidden()
                            .child(list),
                    ),
            ),
        )
    }

    pub(super) fn render_composer(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        div()
            .flex_none()
            .w_full()
            .min_w_0()
            .px(px(20.0))
            .child(self.render_composer_card(window, cx))
    }

    pub(super) fn render_composer_card(&self, window: &Window, cx: &mut Context<Self>) -> Div {
        let theme = Theme::current(cx);
        let session = self.selected_session();
        let preparing = session.is_some_and(|session| {
            self.submission_preparations.contains(&session.id)
                || self.response_fork_preparations.contains_key(&session.id)
        });
        let submit_action =
            composer_submit_action(session.map(|session| session.status), preparing);
        let escape_stop_armed = session.is_some_and(|session| {
            self.escape_stop_confirmation
                .is_armed_for(EscapeStopTarget::for_session(session), Instant::now())
        });
        let has_draft = !self.composer.read(cx).content(cx).trim().is_empty()
            || !self.composer_attachments.is_empty();
        // With no provider to run it, a draft has nowhere to go. The button
        // reads as unavailable and the submission path refuses too, so
        // `enter` cannot slip past a disabled control.
        let no_providers = self.model_picker_has_no_providers();
        let can_send = has_draft && !no_providers;
        let (autocomplete, autocomplete_actionable) =
            match self.render_composer_autocomplete(window, cx) {
                Some((element, actionable)) => (Some(element), actionable),
                None => (None, false),
            };
        let autocomplete_loading = autocomplete.is_some() && !autocomplete_actionable;
        // Files dragged in from the OS light the card up as a drop target and
        // stage as attachment chips. The wash arrives pre-blended because a
        // drag-over refinement replaces the card's fill rather than
        // compositing over it.
        let drop_wash = theme.composer.blend(theme.overlay_strong);
        let drop_ring = theme.accent.opacity(0.7);
        div()
            .w_full()
            .max_w(px(CONTENT_MAX_WIDTH))
            .mx_auto()
            .rounded(px(13.0))
            .border_1()
            .border_color(theme.border)
            .bg(theme.composer)
            .shadow_lg()
            // Horizontal insets live on each row (and inside the field's
            // scroll viewport, via `padding_x`) rather than on the card,
            // so the field's overlay scrollbar can hug the card's edge.
            .py(px(10.0))
            .drag_over::<ExternalPaths>(move |style, _, _, _| {
                style.bg(drop_wash).border_color(drop_ring)
            })
            .on_drop(cx.listener(|this, paths: &ExternalPaths, window, cx| {
                this.stage_dropped_files(paths, window, cx);
            }))
            // Anchor for the bounds probe the autocomplete popup aligns to.
            .relative()
            .child(super::autocomplete::composer_card_bounds_probe(
                self.composer_autocomplete.card_bounds_cell(),
            ))
            // Only while the popup has selectable rows: the key context
            // routes arrows, `enter`, `tab` and `escape` here as actions,
            // out from under the focused field. The loading state takes
            // only Escape, so it can dismiss without swallowing input.
            .when(autocomplete_actionable, |card| {
                card.key_context("ComposerAutocomplete")
                    .on_action(cx.listener(|this, _: &SelectNextEntry, window, cx| {
                        this.move_autocomplete_highlight("down", window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &SelectPreviousEntry, window, cx| {
                        this.move_autocomplete_highlight("up", window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &ConfirmEntry, window, cx| {
                        this.accept_autocomplete(None, window, cx);
                    }))
                    .on_action(cx.listener(|this, _: &DismissMenu, _, cx| {
                        this.dismiss_autocomplete(cx);
                    }))
            })
            .when(autocomplete_loading, |card| {
                card.key_context("ComposerAutocompleteLoading")
                    .on_action(cx.listener(|this, _: &DismissMenu, _, cx| {
                        this.dismiss_autocomplete(cx);
                    }))
            })
            .children(autocomplete)
            .children(self.render_file_editor_selection_badge(cx))
            .when(!self.composer_attachments.is_empty(), |card| {
                card.child(self.render_composer_attachments(cx))
            })
            .child(div().pt(px(2.0)).child(self.composer.clone()))
            .child(
                div()
                    .mt(px(8.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(4.0))
                    .text_size(sp(12.5))
                    .line_height(sp(14.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .overflow_hidden()
                            .child(self.render_provider_model_control(cx))
                            .children(self.render_pi_plan_mode_control(cx))
                            .children(self.render_goal_control(cx)),
                    )
                    .child(
                        div().flex_none().flex().items_center().gap(px(4.0)).child(
                            match submit_action {
                                ComposerSubmitAction::Preparing => div()
                                    .id("send-or-stop")
                                    .flex_none()
                                    .w(px(26.0))
                                    .h(px(26.0))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_default()
                                    .bg(theme.overlay_strong)
                                    .child(dot_matrix_loader(theme.text_secondary, 15.0))
                                    .tooltip(Tooltip::text(tr!("composer.preparing_task"))),
                                ComposerSubmitAction::Stop => div()
                                    .id("working-actions")
                                    .flex_none()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        div()
                                            .id("send-or-stop")
                                            .flex_none()
                                            .w(px(26.0))
                                            .h(px(26.0))
                                            .rounded_full()
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .cursor_default()
                                            .bg(theme.overlay_strong)
                                            .hover(|element| element.bg(theme.danger_soft))
                                            .active(|element| element.opacity(0.8))
                                            .when(escape_stop_armed, |element| {
                                                element.child(
                                                    div()
                                                        .text_size(sp(12.5))
                                                        .font_weight(FontWeight::SEMIBOLD)
                                                        .text_color(theme.text)
                                                        .child("Esc"),
                                                )
                                            })
                                            .when(!escape_stop_armed, |element| {
                                                element.child(icon(
                                                    "icons/stop.svg",
                                                    18.0,
                                                    theme.text,
                                                ))
                                            })
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.cancel_turn(cx);
                                            })),
                                    )
                                    .when(can_send, |element| {
                                        element.child(
                                            div()
                                                .id("queue-follow-up")
                                                .flex_none()
                                                .w(px(26.0))
                                                .h(px(26.0))
                                                .rounded_full()
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .cursor_default()
                                                .bg(theme.inverse)
                                                .hover(|element| element.opacity(0.9))
                                                .active(|element| element.opacity(0.8))
                                                .focus_visible(|element| {
                                                    element.border_1().border_color(theme.accent)
                                                })
                                                .tab_index(0)
                                                .on_key_down(cx.listener(
                                                    |this, event: &KeyDownEvent, _, cx| {
                                                        if matches!(
                                                            event.keystroke.key.as_str(),
                                                            "enter" | "space"
                                                        ) {
                                                            let prompt = this
                                                                .composer
                                                                .read(cx)
                                                                .content(cx)
                                                                .to_owned();
                                                            if let Some(submission) = this
                                                                .submission_with_attachments(
                                                                    &prompt, cx,
                                                                )
                                                            {
                                                                this.composer
                                                                    .update(cx, |input, cx| {
                                                                        input.clear(cx)
                                                                    });
                                                                this.submit_composer_submission(
                                                                    submission, cx,
                                                                );
                                                            }
                                                            cx.stop_propagation();
                                                        }
                                                    },
                                                ))
                                                .child(icon(
                                                    "icons/arrow-up.svg",
                                                    16.0,
                                                    theme.on_inverse,
                                                ))
                                                .tooltip(Tooltip::text(tr!(
                                                    "composer.queue_followup"
                                                )))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    let prompt = this
                                                        .composer
                                                        .read(cx)
                                                        .content(cx)
                                                        .to_owned();
                                                    if let Some(submission) = this
                                                        .submission_with_attachments(&prompt, cx)
                                                    {
                                                        this.composer.update(cx, |input, cx| {
                                                            input.clear(cx)
                                                        });
                                                        this.submit_composer_submission(
                                                            submission, cx,
                                                        );
                                                    }
                                                })),
                                        )
                                    }),
                                ComposerSubmitAction::Send => div()
                                    .id("send-or-stop")
                                    .flex_none()
                                    .w(px(26.0))
                                    .h(px(26.0))
                                    .rounded_full()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .bg(if can_send {
                                        theme.inverse
                                    } else {
                                        theme.overlay_strong
                                    })
                                    .when(can_send, |element| {
                                        element
                                            .cursor_default()
                                            .hover(|element| element.opacity(0.9))
                                            .active(|element| element.opacity(0.8))
                                    })
                                    .child(icon(
                                        "icons/arrow-up.svg",
                                        16.0,
                                        if can_send {
                                            theme.on_inverse
                                        } else {
                                            theme.text_ghost
                                        },
                                    ))
                                    // Says why the button is dead, for the case
                                    // the draft is ready and the machine is not.
                                    .when(no_providers, |element| {
                                        element.tooltip(Tooltip::text(tr!("composer.no_providers")))
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        let prompt = this.composer.read(cx).content(cx).to_owned();
                                        if let Some(submission) =
                                            this.submission_with_attachments(&prompt, cx)
                                        {
                                            this.composer.update(cx, |input, cx| input.clear(cx));
                                            this.submit_composer_submission(submission, cx);
                                        }
                                    })),
                            },
                        ),
                    ),
            )
    }

    fn render_branch_selector(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let theme = Theme::current(cx);
        let session = self.selected_session();
        let workspace = session.map(|s| s.workspace.clone()).unwrap_or_default();
        let workspace_path = if let Some(session) = session {
            self.workspace_path_for_session(session)?.to_path_buf()
        } else {
            self.selected_project()?.path.clone()
        };
        self.selected_project()
            .filter(|project| !project.is_projectless())?;
        let branch_enabled =
            session.map(|s| !s.is_busy()).unwrap_or(true) && !self.branch_operation_pending;
        let planned_worktree = matches!(workspace, SessionWorkspace::NewWorktree { .. });
        let snapshot = self.branch_snapshot_for_workspace(&workspace_path, cx)?;
        let selected_branch = match &workspace {
            SessionWorkspace::Local => snapshot.display_branch().map(str::to_owned),
            SessionWorkspace::NewWorktree { base_branch } => base_branch
                .clone()
                .or_else(|| snapshot.default_branch.clone())
                .or_else(|| snapshot.display_branch().map(str::to_owned)),
            SessionWorkspace::Worktree { branch, .. } => snapshot
                .current
                .clone()
                .or_else(|| Some(branch.clone()))
                .or_else(|| snapshot.detached_head.clone()),
        }
        .unwrap_or_else(|| tr!("branches.detached_head"));

        let weak = cx.entity().downgrade();
        let search = self.branch_search.clone();
        let create_input = self.branch_create_input.clone();
        let search_focus = search.read(cx).focus_handle(cx);
        let handle = {
            let toggle_weak = weak.clone();
            let reset_search = search.clone();
            let reset_create = create_input.clone();
            let picker_focus = search_focus.clone();
            self.menu_handle_with(BRANCH_PICKER_MENU_ID, cx, move |open, window, cx| {
                let _ = toggle_weak.update(cx, |this, cx| {
                    if open {
                        this.branch_picker_mode = BranchPickerMode::Browse;
                        this.branch_picker_highlight = None;
                        let project_name = this
                            .selected_project()
                            .map(Project::display_name)
                            .unwrap_or_else(|| tr!("project.project_lower"));
                        reset_search.update(cx, |input, cx| {
                            input.set_placeholder(
                                tr!("branches.search_project", project = project_name),
                                cx,
                            );
                            input.clear(cx);
                        });
                        reset_create.update(cx, |input, cx| input.clear(cx));
                        this.refresh_selected_branch_snapshot(cx);
                    } else {
                        this.branch_picker_mode = BranchPickerMode::Browse;
                        let focus = this.composer_focus(cx);
                        window.focus(&focus, cx);
                    }
                    cx.notify();
                });
                if open {
                    let picker_focus = picker_focus.clone();
                    window.on_next_frame(move |window, _| {
                        window.on_next_frame(move |window, cx| window.focus(&picker_focus, cx));
                    });
                }
            })
        };

        let trigger = MenuChip::new("workspace-branch")
            .icon("icons/git-branch.svg", theme.text_tertiary)
            .label(if self.branch_operation_pending {
                tr!("branches.switching")
            } else {
                selected_branch.clone()
            })
            .caret(false)
            .disabled(!branch_enabled)
            .selected(branch_enabled && handle.is_open())
            .max_w(px(210.0));
        if !branch_enabled {
            return Some(trigger.into_any_element());
        }

        let normalized_query = self
            .branch_search
            .read(cx)
            .content()
            .trim()
            .to_ascii_lowercase();
        let visible_branches = Rc::new(
            if handle.is_open() && self.branch_picker_mode == BranchPickerMode::Browse {
                visible_branch_entries(&snapshot.branches, &selected_branch, &normalized_query)
            } else {
                Vec::new()
            },
        );
        let allow_create = !planned_worktree;
        let actions = Rc::new(
            visible_branches
                .iter()
                .filter(|branch| planned_worktree || !branch.checked_out_elsewhere)
                .map(|branch| BranchPickerAction::Checkout(branch.name.clone()))
                .chain(allow_create.then_some(BranchPickerAction::Create))
                .collect::<Vec<_>>(),
        );
        let highlight = self
            .branch_picker_highlight
            .filter(|index| *index < actions.len());
        let mode = self.branch_picker_mode;
        if handle.is_open() && mode == BranchPickerMode::Browse {
            self.sync_branch_picker_rows(&visible_branches);
        }
        let branch_list = self.branch_picker_list_state.clone();

        Some(popover(
            trigger,
            &handle,
            MenuAlign::AboveLeft,
            move |popover, _window, _cx| {
                let popover = popover.clone();
                let next_actions = actions.clone();
                let previous_actions = actions.clone();
                let confirm_actions = actions.clone();
                let dismiss_weak = weak.clone();
                let next_weak = weak.clone();
                let previous_weak = weak.clone();
                let confirm_weak = weak.clone();
                let confirm_popover = popover.clone();

                let body = if mode == BranchPickerMode::Create {
                    div()
                        .w_full()
                        .p(px(14.0))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .text_size(sp(13.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text)
                                .child(icon("icons/plus.svg", 14.0, theme.text_secondary))
                                .child(tr!("branches.create_and_checkout")),
                        )
                        .child(
                            div()
                                .mt(px(12.0))
                                .h(px(36.0))
                                .px(px(10.0))
                                .rounded(px(9.0))
                                .border_1()
                                .border_color(theme.border_strong)
                                .bg(theme.elevated_surface)
                                .flex()
                                .items_center()
                                .child(div().flex_1().min_w_0().child(create_input.clone())),
                        )
                        .child(
                            div()
                                .mt(px(9.0))
                                .text_size(sp(12.5))
                                .text_color(theme.text_tertiary)
                                .child(tr!("branches.create_hint")),
                        )
                        .into_any_element()
                } else {
                    let rows = if visible_branches.is_empty() {
                        div()
                            .id("branch-picker-list-empty")
                            .h(px(64.0))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(sp(12.5))
                            .text_color(theme.text_ghost)
                            .child(tr!("branches.none_found"))
                            .into_any_element()
                    } else {
                        let list_branches = visible_branches.clone();
                        let list_actions = actions.clone();
                        let list_selected_branch = selected_branch.clone();
                        let list_weak = weak.clone();
                        let list_popover = popover.clone();
                        let height =
                            (visible_branches.len() as f32 * BRANCH_PICKER_ROW_HEIGHT).min(260.0);
                        div()
                            .id("branch-picker-list")
                            .w_full()
                            .h(px(height))
                            .flex_none()
                            .px(px(4.0))
                            .child(
                                list(branch_list.clone(), move |index, _window, _cx| {
                                    let Some(branch) = list_branches.get(index) else {
                                        return div().into_any_element();
                                    };
                                    let selected = branch.name == list_selected_branch;
                                    let disabled =
                                        branch.checked_out_elsewhere && !planned_worktree;
                                    let highlighted = highlight
                                        .and_then(|index| list_actions.get(index))
                                        .is_some_and(|action| {
                                            matches!(
                                                action,
                                                BranchPickerAction::Checkout(name)
                                                    if name == &branch.name
                                            )
                                        });
                                    let color = if disabled {
                                        theme.text_ghost
                                    } else {
                                        theme.text
                                    };
                                    let row = div()
                                        .id(SharedString::from(format!(
                                            "branch-row-{}",
                                            branch.name
                                        )))
                                        .w_full()
                                        .h(px(BRANCH_PICKER_ROW_HEIGHT))
                                        .px(px(8.0))
                                        .rounded(px(6.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .cursor_default()
                                        .when(highlighted, |element| {
                                            element.bg(theme.overlay_strong)
                                        })
                                        .when(!disabled, |element| {
                                            element
                                                .hover(|element| element.bg(theme.overlay))
                                                .active(|element| element.opacity(0.85))
                                        })
                                        .child(icon("icons/git-branch.svg", 12.0, color))
                                        .child(
                                            div()
                                                .min_w_0()
                                                .flex_1()
                                                .truncate()
                                                .text_size(sp(12.5))
                                                .line_height(sp(15.0))
                                                .text_color(color)
                                                .child(SharedString::from(branch.name.clone())),
                                        )
                                        .when(selected, |element| {
                                            element.child(icon(
                                                "icons/check.svg",
                                                11.0,
                                                theme.text_secondary,
                                            ))
                                        });
                                    if disabled {
                                        row.into_any_element()
                                    } else {
                                        let branch_name = branch.name.clone();
                                        let select_weak = list_weak.clone();
                                        let select_popover = list_popover.clone();
                                        row.on_click(move |_, window, cx| {
                                            let should_close = select_weak
                                                .update(cx, |this, cx| {
                                                    this.choose_workspace_branch(
                                                        branch_name.clone(),
                                                        cx,
                                                    )
                                                })
                                                .unwrap_or(false);
                                            if should_close {
                                                select_popover.close(window, cx);
                                                window.refresh();
                                            }
                                        })
                                        .into_any_element()
                                    }
                                })
                                .size_full(),
                            )
                            .into_any_element()
                    };

                    let create_row = allow_create.then(|| {
                        let create_weak = weak.clone();
                        div()
                            .id("create-workspace-branch")
                            .mx(px(4.0))
                            .h(px(BRANCH_PICKER_ROW_HEIGHT))
                            .px(px(8.0))
                            .rounded(px(6.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .cursor_default()
                            .when(
                                highlight.and_then(|index| actions.get(index))
                                    == Some(&BranchPickerAction::Create),
                                |element| element.bg(theme.overlay_strong),
                            )
                            .hover(|element| element.bg(theme.overlay))
                            .active(|element| element.opacity(0.85))
                            .child(icon("icons/plus.svg", 12.0, theme.text_secondary))
                            .child(
                                div()
                                    .text_size(sp(12.5))
                                    .line_height(sp(15.0))
                                    .text_color(theme.text)
                                    .child(tr!("branches.create_and_checkout_ellipsis")),
                            )
                            .on_click(move |_, window, cx| {
                                let _ = create_weak.update(cx, |this, cx| {
                                    this.begin_branch_creation(window, cx);
                                });
                            })
                    });

                    div()
                        .w_full()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .h(px(52.0))
                                .px(px(12.0))
                                .pt(px(10.0))
                                .pb(px(8.0))
                                .flex_none()
                                .flex()
                                .items_center()
                                .child(
                                    div()
                                        .w_full()
                                        .h(px(34.0))
                                        .px(px(10.0))
                                        .rounded(px(9.0))
                                        .border_1()
                                        .border_color(theme.border)
                                        .bg(theme.elevated_surface)
                                        .flex()
                                        .items_center()
                                        .gap(px(8.0))
                                        .child(icon("icons/search.svg", 15.0, theme.text_secondary))
                                        .child(div().flex_1().min_w_0().child(search.clone())),
                                ),
                        )
                        .child(
                            div()
                                .px(px(14.0))
                                .pt(px(3.0))
                                .pb(px(7.0))
                                .text_size(sp(12.5))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(theme.text_tertiary)
                                .child(tr!("branches.title")),
                        )
                        .child(rows)
                        .when_some(create_row, |element, create_row| {
                            element
                                .child(div().mx(px(6.0)).my(px(4.0)).h(px(1.0)).bg(theme.border))
                                .child(create_row)
                                .child(div().h(px(4.0)))
                        })
                        .into_any_element()
                };

                div()
                    .w(px(360.0))
                    .max_h(px(390.0))
                    .rounded(px(13.0))
                    .overflow_hidden()
                    .border_1()
                    .border_color(theme.border_strong)
                    .bg(theme.elevated)
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .on_action(move |_: &SelectNextEntry, _, cx| {
                        let _ = next_weak.update(cx, |this, cx| {
                            this.move_branch_picker_highlight("down", &next_actions, cx);
                        });
                    })
                    .on_action(move |_: &SelectPreviousEntry, _, cx| {
                        let _ = previous_weak.update(cx, |this, cx| {
                            this.move_branch_picker_highlight("up", &previous_actions, cx);
                        });
                    })
                    .on_action(move |_: &ConfirmEntry, window, cx| {
                        let should_close = confirm_weak
                            .update(cx, |this, cx| {
                                this.confirm_branch_picker_action(&confirm_actions, window, cx)
                            })
                            .unwrap_or(false);
                        if should_close {
                            confirm_popover.close(window, cx);
                            window.refresh();
                        }
                    })
                    // Escape backs the create form out to browsing. The rest
                    // of the peel is the fields' own clear-on-escape: a
                    // non-empty filter (or typed branch name) clears before
                    // this handler ever sees the keystroke, and an empty
                    // browse view propagates on to the menu's own dismiss.
                    .on_action(move |_: &DismissMenu, window, cx| {
                        let handled = dismiss_weak
                            .update(cx, |this, cx| {
                                if this.branch_picker_mode == BranchPickerMode::Create {
                                    this.cancel_branch_creation(window, cx);
                                    return true;
                                }
                                false
                            })
                            .unwrap_or(false);
                        if !handled {
                            cx.propagate();
                        }
                    })
                    .child(body)
                    .into_any_element()
            },
        ))
    }

    pub(super) fn render_workspace_footer(&mut self, cx: &mut Context<Self>) -> Div {
        let theme = Theme::current(cx);
        let selected_project_id = self.state.selected_project;
        let projectless_selected = self.selected_project().is_some_and(Project::is_projectless);
        let project_name = self
            .selected_project()
            .map(|project| {
                if project.is_projectless() {
                    tr!("project.choose_project")
                } else {
                    project.display_name()
                }
            })
            .unwrap_or_else(|| tr!("project.choose_project"));
        let can_configure_workspace = self
            .selected_session()
            .map(|session| !session.has_started() && !session.is_busy())
            .unwrap_or(true);

        let project_handle = self.menu_handle("workspace-project", cx);
        let project_trigger = MenuChip::new("workspace-project")
            .icon("icons/folder.svg", theme.text_tertiary)
            .label(project_name)
            .caret(false)
            .disabled(!can_configure_workspace)
            .selected(can_configure_workspace && project_handle.is_open())
            .max_w(px(190.0));
        let project_selector = if can_configure_workspace {
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
            dropdown_menu(
                project_trigger,
                "workspace-project-menu",
                &project_handle,
                MenuAlign::AboveLeft,
                move |_| {
                    let mut items = project_options
                        .clone()
                        .into_iter()
                        .map(|(project_id, project_name)| {
                            let weak = weak.clone();
                            MenuItem::new(project_name, move |_, cx| {
                                if Some(project_id) != selected_project_id {
                                    let _ = weak.update(cx, |this, cx| {
                                        this.select_project_from_composer(project_id, cx);
                                    });
                                }
                            })
                            .selected(Some(project_id) == selected_project_id)
                        })
                        .collect::<Vec<_>>();
                    if !items.is_empty() {
                        items.push(MenuItem::Separator);
                    }
                    let add_project = weak.clone();
                    items.push(
                        MenuItem::new(tr!("project.new_project"), move |window, cx| {
                            let _ = add_project
                                .update(cx, |this, cx| this.open_project_source_dialog(window, cx));
                        })
                        .icon("icons/folder-new.svg"),
                    );
                    let projectless = weak.clone();
                    items.push(
                        MenuItem::new(tr!("project.no_project"), move |_, cx| {
                            let _ = projectless.update(cx, |this, cx| {
                                if !this.selected_project().is_some_and(Project::is_projectless) {
                                    this.create_projectless_session_from_composer(cx);
                                }
                            });
                        })
                        .icon("icons/x.svg")
                        .selected(projectless_selected),
                    );
                    items
                },
            )
        } else {
            project_trigger.into_any_element()
        };

        let workspace = self
            .selected_session()
            .map(|session| session.workspace.clone())
            .unwrap_or_default();
        let workspace_label = match &workspace {
            SessionWorkspace::Local => SharedString::from(tr!("workspace.local")),
            SessionWorkspace::NewWorktree { .. } => {
                SharedString::from(tr!("workspace.new_worktree"))
            }
            SessionWorkspace::Worktree { branch, .. } => SharedString::from(branch.clone()),
        };
        let workspace_icon = if workspace.is_local() {
            "icons/laptop.svg"
        } else {
            "icons/fork.svg"
        };
        let worktree_handle = self.menu_handle("workspace-worktree", cx);
        let worktree_trigger = MenuChip::new("workspace-worktree")
            .icon(workspace_icon, theme.text_tertiary)
            .label(workspace_label)
            .caret(false)
            .disabled(!can_configure_workspace)
            .selected(can_configure_workspace && worktree_handle.is_open())
            .max_w(px(180.0));
        let worktree_selector = if can_configure_workspace {
            let local_selected = workspace.is_local();
            let worktree_selected = workspace.is_worktree();
            let weak = cx.entity().downgrade();
            dropdown_menu(
                worktree_trigger,
                "workspace-worktree-menu",
                &worktree_handle,
                MenuAlign::AboveLeft,
                move |_| {
                    let local = weak.clone();
                    let worktree = weak.clone();
                    vec![
                        MenuItem::Header(tr!("workspace.work_in").into()),
                        MenuItem::new(tr!("workspace.local"), move |_, cx| {
                            let _ = local.update(cx, |this, cx| {
                                this.select_workspace(SessionWorkspace::Local, cx);
                            });
                        })
                        .icon("icons/laptop.svg")
                        .selected(local_selected),
                        MenuItem::new(tr!("workspace.new_worktree"), move |_, cx| {
                            let _ = worktree.update(cx, |this, cx| {
                                this.select_workspace(
                                    SessionWorkspace::NewWorktree { base_branch: None },
                                    cx,
                                );
                            });
                        })
                        .icon("icons/fork.svg")
                        .selected(worktree_selected)
                        .disabled(projectless_selected),
                    ]
                },
            )
        } else {
            worktree_trigger.into_any_element()
        };

        let branch_selector = self.render_branch_selector(cx);

        let usage_meter = self.render_usage_meter(cx);
        div()
            .flex_none()
            .px(px(20.0))
            .pb(px(8.0))
            .pt(px(4.0))
            .child(
                div()
                    .w_full()
                    .max_w(px(CONTENT_MAX_WIDTH))
                    .mx_auto()
                    .h(px(28.0))
                    // The chip contributes 7px, lining its icon up with the
                    // composer's 10px padding plus the controls' 7px inset.
                    .pl(px(10.0))
                    .pr(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .tab_index(0)
                    .tab_group()
                    .tab_stop(false)
                    .text_size(sp(12.5))
                    .line_height(sp(14.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .overflow_hidden()
                            .child(project_selector)
                            .child(worktree_selector)
                            .children(branch_selector),
                    )
                    .child(div().flex_none().children(usage_meter)),
            )
    }
}

/// Branches matching the search, with the selected branch pinned first and
/// every other row sorted by name. Disabled worktree-owned rows stay in the
/// result; the UI needs to explain why Git cannot switch to them.
pub(super) fn visible_branch_entries(
    branches: &[crate::git_branch::BranchEntry],
    selected_branch: &str,
    normalized_query: &str,
) -> Vec<crate::git_branch::BranchEntry> {
    let normalized_query = normalized_query.to_ascii_lowercase();
    let mut visible = branches
        .iter()
        .filter(|branch| {
            normalized_query
                .split_whitespace()
                .all(|token| branch.name.to_ascii_lowercase().contains(token))
        })
        .cloned()
        .collect::<Vec<_>>();
    visible.sort_by(|left, right| {
        let left_selected = left.name == selected_branch;
        let right_selected = right.name == selected_branch;
        right_selected
            .cmp(&left_selected)
            .then_with(|| left.name.cmp(&right.name))
    });
    visible
}

/// The mention a dropped file submits: relative to the project root when the
/// file is inside it, absolute otherwise, directories with a trailing slash —
/// the same form the `@` autocomplete inserts. Dropping the root itself keeps
/// the absolute path rather than producing an empty mention.
// Base64 keeps the authenticated JSON transport browser-compatible but adds
// one third of wire overhead. Stay comfortably below tungstenite's default
// message limit until uploads move to a streaming content endpoint.
const MAX_ATTACHMENT_BYTES: u64 = insulator_client::attachments::MAX_ATTACHMENT_BYTES as u64;

/// Reads a client-local drop into an upload payload. This is the explicit
/// client/daemon boundary: none of these source paths are persisted or handed
/// to a provider.
fn attachment_upload_from_path(
    source: &Path,
) -> anyhow::Result<(
    String,
    insulator_client::attachments::AttachmentUpload,
    Option<Vec<u8>>,
)> {
    let metadata = std::fs::symlink_metadata(source)
        .with_context(|| format!("could not read attachment {}", source.display()))?;
    if metadata.file_type().is_symlink() {
        anyhow::bail!(
            "symbolic-link attachments are not supported: {}",
            source.display()
        );
    }
    let name = source
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow::anyhow!("attachment has no file name: {}", source.display()))?
        .to_owned();
    if metadata.is_file() {
        if metadata.len() > MAX_ATTACHMENT_BYTES {
            anyhow::bail!("attachment is larger than 32 MB: {}", source.display());
        }
        let bytes = std::fs::read(source)
            .with_context(|| format!("could not read attachment {}", source.display()))?;
        let is_image = is_image_attachment_path(source);
        return Ok((
            name,
            insulator_client::attachments::AttachmentUpload::File {
                data_base64: base64::engine::general_purpose::STANDARD.encode(&bytes),
            },
            is_image.then_some(bytes),
        ));
    }
    if !metadata.is_dir() {
        anyhow::bail!(
            "attachment is not a file or directory: {}",
            source.display()
        );
    }

    let mut pending = vec![source.to_path_buf()];
    let mut entries = Vec::new();
    let mut total_bytes = 0u64;
    while let Some(directory) = pending.pop() {
        for entry in std::fs::read_dir(&directory).with_context(|| {
            format!(
                "could not read attachment directory {}",
                directory.display()
            )
        })? {
            let entry = entry?;
            let path = entry.path();
            let metadata = std::fs::symlink_metadata(&path)?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if !metadata.is_file() {
                continue;
            }
            if entries.len() >= insulator_client::attachments::MAX_ATTACHMENT_FILES {
                anyhow::bail!(
                    "attachment directory contains more than {} files",
                    insulator_client::attachments::MAX_ATTACHMENT_FILES
                );
            }
            total_bytes = total_bytes.saturating_add(metadata.len());
            if total_bytes > MAX_ATTACHMENT_BYTES {
                anyhow::bail!("attachment directory is larger than 32 MB");
            }
            let relative_path = path
                .strip_prefix(source)
                .context("attachment entry escaped its source directory")?
                .to_path_buf();
            let bytes = std::fs::read(&path)
                .with_context(|| format!("could not read attachment {}", path.display()))?;
            entries.push(insulator_client::attachments::AttachmentUploadEntry {
                relative_path,
                data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
            });
        }
    }
    Ok((
        name,
        insulator_client::attachments::AttachmentUpload::Directory { entries },
        None,
    ))
}

#[cfg(test)]
pub(super) fn dropped_file_mention(
    root: Option<&std::path::Path>,
    path: &std::path::Path,
    is_dir: bool,
) -> String {
    let mention = root
        .and_then(|root| path.strip_prefix(root).ok())
        .filter(|relative| !relative.as_os_str().is_empty())
        .unwrap_or(path)
        .display()
        .to_string();
    if is_dir && !mention.ends_with('/') {
        format!("{mention}/")
    } else {
        mention
    }
}

fn is_image_attachment_path(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png"
                    | "jpg"
                    | "jpeg"
                    | "gif"
                    | "webp"
                    | "bmp"
                    | "svg"
                    | "tif"
                    | "tiff"
                    | "ico"
                    | "pnm"
                    | "pbm"
                    | "pgm"
                    | "ppm"
            )
        })
}

/// The prompt a submission sends: the typed text plus one `@` mention per
/// staged attachment, appended at the end the way T3 Code appends dropped
/// files. `None` means there is nothing to send.
pub(super) fn merged_submission(prompt: &str, mentions: &[String]) -> Option<String> {
    let mentions = mentions
        .iter()
        .map(|mention| format!("@{mention}"))
        .collect::<Vec<_>>()
        .join(" ");
    let prompt = prompt.trim();
    match (prompt.is_empty(), mentions.is_empty()) {
        (true, true) => None,
        (false, true) => Some(prompt.to_owned()),
        (true, false) => Some(mentions),
        (false, false) => Some(format!("{prompt} {mentions}")),
    }
}

/// Where the picker's keyboard cursor lands, wrapping at both ends.
///
/// `None` for `current` means the cursor has not moved yet, so `down` opens on
/// the first row and `up` on the last. `None` in the result means the key does
/// not navigate.
pub(super) fn next_picker_highlight(
    current: Option<usize>,
    len: usize,
    key: &str,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    match key {
        "down" => Some(current.map_or(0, |index| (index + 1) % len)),
        "up" => Some(current.map_or(len - 1, |index| (index + len - 1) % len)),
        _ => None,
    }
}

/// The sidebar tabs the picker can land on, in rail order: favorites first,
/// then every installed provider a new session may use.
///
/// Shared by the rail's click gating and by `tab`'s cycle handler so the two
/// agree on which tabs are usable. A locked session keeps its own provider
/// usable even if it was switched off afterwards — disabling is for new work —
/// while every other provider drops out for the lock's duration.
pub(super) fn visible_picker_tabs(
    probes: &[ProviderProbe],
    disabled_providers: &[ProviderKind],
    locked_provider: Option<ProviderKind>,
) -> Vec<ModelPickerTab> {
    let mut tabs = vec![ModelPickerTab::Recents, ModelPickerTab::Favorites];
    tabs.extend(ProviderKind::ALL.into_iter().filter_map(|kind| {
        let allowed = locked_provider.is_none() || locked_provider == Some(kind);
        let drawn = picker_rail_shows_provider(probes, disabled_providers, locked_provider, kind);
        (allowed && drawn).then_some(ModelPickerTab::Provider(kind))
    }));
    tabs
}

/// The picker's whole body when nothing can back a session: no agent CLI
/// found on this machine, and none left switched on.
///
/// A rail holding a lone star above an empty filter field would invite the
/// user to search a list that cannot have rows, so the panel names what is
/// missing and offers the page that fixes it. Its one button also carries the
/// panel's focus, which is what `escape` dispatches up from.
fn model_picker_empty_state(
    theme: &Theme,
    focus: &FocusHandle,
    popover: ContextMenuHandle,
    insulator: WeakEntity<Insulator>,
) -> AnyElement {
    let click_popover = popover.clone();
    let click_insulator = insulator.clone();
    div()
        .w(px(320.0))
        .rounded(px(13.0))
        .overflow_hidden()
        .border_1()
        .border_color(theme.border_strong)
        .bg(theme.elevated)
        .shadow_lg()
        .flex()
        .flex_col()
        .items_center()
        .gap(px(9.0))
        .px(px(24.0))
        .py(px(22.0))
        .child(
            div()
                .w(px(40.0))
                .h(px(40.0))
                .rounded(px(10.0))
                .bg(theme.overlay)
                .flex()
                .items_center()
                .justify_center()
                .child(icon("icons/bot.svg", 19.0, theme.text_tertiary)),
        )
        .child(
            div()
                .text_size(sp(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(theme.text)
                .child(tr!("models.no_providers_title")),
        )
        .child(
            div()
                .text_size(sp(12.5))
                .line_height(sp(17.0))
                .text_center()
                .text_color(theme.text_secondary)
                .child(tr!("models.no_providers_description")),
        )
        .child(
            div()
                .id("model-picker-open-providers")
                .track_focus(focus)
                .tab_index(0)
                .tab_stop(true)
                .focus_visible(|style| style.border_color(theme.accent))
                .mt(px(3.0))
                .h(px(28.0))
                .px(px(11.0))
                .rounded(px(7.0))
                .border_1()
                .border_color(theme.border_strong)
                .flex()
                .items_center()
                .gap(px(6.0))
                .cursor_default()
                .text_size(sp(12.5))
                .text_color(theme.text_secondary)
                .hover(|element| element.bg(theme.overlay))
                .child(icon("icons/settings.svg", 11.0, theme.text_tertiary))
                .child(tr!("models.open_provider_settings"))
                .on_click(move |_, window, cx| {
                    open_provider_settings_from_picker(
                        &click_insulator,
                        &click_popover,
                        window,
                        cx,
                    );
                })
                .on_key_down(move |event: &KeyDownEvent, window, cx| {
                    if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                        open_provider_settings_from_picker(&insulator, &popover, window, cx);
                        cx.stop_propagation();
                    }
                }),
        )
        .into_any_element()
}

/// Dismiss the picker and land on the Providers page, for both the empty
/// state's click and its keyboard activation. Closing first matters: the
/// picker returns focus to the composer as it closes, which would otherwise
/// pull focus straight back out of the settings view.
fn open_provider_settings_from_picker(
    insulator: &WeakEntity<Insulator>,
    popover: &ContextMenuHandle,
    window: &mut Window,
    cx: &mut App,
) {
    popover.close(window, cx);
    let _ = insulator.update(cx, |this, cx| {
        this.open_settings_action(&OpenSettings, window, cx);
        this.open_settings_page(SettingsPage::Providers, cx);
    });
}

/// Whether the rail draws a tab for the provider at all, usable or not.
///
/// Installed on this machine and not switched off in the Providers settings.
/// Both of those are settings-level facts the user has already decided, so the
/// tab is absent rather than dimmed — the rail offers what could be picked,
/// not a catalog of everything Insulator can speak to. A session locked to a
/// provider switched off afterwards keeps its own tab, since the picker is
/// that session's only route to another model.
pub(super) fn picker_rail_shows_provider(
    probes: &[ProviderProbe],
    disabled_providers: &[ProviderKind],
    locked_provider: Option<ProviderKind>,
    kind: ProviderKind,
) -> bool {
    if locked_provider == Some(kind) {
        return true;
    }
    let installed = probes
        .iter()
        .any(|probe| probe.provider == kind && probe.installed);
    let switched_off = disabled_providers.contains(&kind);
    installed && !switched_off
}

pub(super) fn model_picker_subtitle(provider: ProviderKind, sub_provider: Option<&str>) -> String {
    let provider_name = provider.short_name();
    match sub_provider.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) if name.eq_ignore_ascii_case(provider_name) => provider_name.to_owned(),
        Some(name) => format!("{name} · {provider_name}"),
        None => provider_name.to_owned(),
    }
}

/// Whether the picker has nothing left to offer, so the composer's trigger
/// and the panel behind it both swap to their empty state.
///
/// `detection_settled` gates the whole answer. Every probe is seeded as "not
/// installed" and detection answers off the UI thread, so a pass that has
/// never completed means "not known yet", never "nothing here" — otherwise
/// the trigger would flash an empty state during every launch.
pub(super) fn picker_has_no_providers(
    probes: &[ProviderProbe],
    disabled_providers: &[ProviderKind],
    locked_provider: Option<ProviderKind>,
    detection_settled: bool,
) -> bool {
    detection_settled
        && !ProviderKind::ALL.into_iter().any(|kind| {
            picker_rail_shows_provider(probes, disabled_providers, locked_provider, kind)
        })
}

/// The models the picker lists, in display order.
///
/// Shared by the panel body and by `enter`'s handler so a keyboard cursor index
/// always means the same row in both.
pub(super) fn visible_picker_models(
    probes: &[ProviderProbe],
    recents: &[FavoriteModel],
    favorites: &[FavoriteModel],
    disabled_providers: &[ProviderKind],
    locked_provider: Option<ProviderKind>,
    selected_tab: ModelPickerTab,
    normalized_query: &str,
) -> Vec<(ProviderKind, ProviderModel)> {
    let searching = !normalized_query.is_empty();
    let mut models = probes
        .iter()
        .filter(|probe| probe.installed || locked_provider == Some(probe.provider))
        .flat_map(|probe| {
            probe
                .models
                .iter()
                .cloned()
                .map(move |model| (probe.provider, model))
        })
        .filter(|(kind, _)| {
            locked_provider.map_or_else(
                || !disabled_providers.contains(kind),
                |locked| locked == *kind,
            )
        })
        .filter(|(kind, model)| {
            if searching {
                let searchable = format!(
                    "{} {} {} {}",
                    model.name,
                    model.id,
                    kind.short_name(),
                    model.sub_provider.as_deref().unwrap_or("")
                )
                .to_ascii_lowercase();
                return normalized_query
                    .split_whitespace()
                    .all(|token| searchable.contains(token));
            }
            match selected_tab {
                ModelPickerTab::Recents => recents
                    .iter()
                    .any(|recent| recent.provider == *kind && recent.model == model.id),
                ModelPickerTab::Favorites => favorites
                    .iter()
                    .any(|favorite| favorite.provider == *kind && favorite.model == model.id),
                ModelPickerTab::Provider(provider) => provider == *kind,
            }
        })
        .collect::<Vec<_>>();

    if !searching && selected_tab == ModelPickerTab::Recents {
        for recent in recents {
            let provider_allowed = locked_provider.map_or_else(
                || !disabled_providers.contains(&recent.provider),
                |locked| locked == recent.provider,
            );
            if provider_allowed {
                if !models
                    .iter()
                    .any(|(kind, model)| *kind == recent.provider && model.id == recent.model)
                {
                    models.push((
                        recent.provider,
                        ProviderModel::new(&recent.model, &recent.model),
                    ));
                }
            }
        }
        models.sort_by_key(|(kind, model)| {
            recents
                .iter()
                .position(|recent| recent.provider == *kind && recent.model == model.id)
                .unwrap_or(usize::MAX)
        });
    }

    if !searching && selected_tab == ModelPickerTab::Favorites {
        models.sort_by_key(|(kind, model)| {
            favorites
                .iter()
                .position(|favorite| favorite.provider == *kind && favorite.model == model.id)
                .unwrap_or(usize::MAX)
        });
    }
    models
}
