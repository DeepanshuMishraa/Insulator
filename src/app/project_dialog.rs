//! Project source chooser and GitHub repository URL dialog.

use std::path::PathBuf;
use std::time::Duration;

use gpui::{KeyBinding, actions};
use serde::{Deserialize, Serialize};

use super::*;

actions!(
    insulator_project_dialog,
    [
        ConfirmGithubProject,
        DismissProjectDialog,
        ConfirmRename,
        DismissRename
    ]
);

const DIALOG_CONTEXT: &str = "ProjectDialog";
const DIALOG_INPUT_CONTEXT: &str = "ProjectDialog > TextInput";
const RENAME_DIALOG_CONTEXT: &str = "RenameDialog";
const RENAME_INPUT_CONTEXT: &str = "RenameDialog > TextInput";

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", ConfirmGithubProject, Some(DIALOG_INPUT_CONTEXT)),
        KeyBinding::new(
            "cmd-enter",
            ConfirmGithubProject,
            Some(DIALOG_INPUT_CONTEXT),
        ),
        KeyBinding::new("enter", ConfirmGithubProject, Some(DIALOG_CONTEXT)),
        KeyBinding::new("cmd-enter", ConfirmGithubProject, Some(DIALOG_CONTEXT)),
        KeyBinding::new("escape", DismissProjectDialog, Some(DIALOG_CONTEXT)),
        KeyBinding::new("enter", ConfirmRename, Some(RENAME_INPUT_CONTEXT)),
        KeyBinding::new("enter", ConfirmRename, Some(RENAME_DIALOG_CONTEXT)),
        KeyBinding::new("escape", DismissRename, Some(RENAME_DIALOG_CONTEXT)),
    ]);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum RenameTarget {
    Project(Uuid),
    Session(Uuid),
}

pub(super) struct RenameDialogState {
    pub target: RenameTarget,
    pub input: Entity<TextInput>,
    pub focus: FocusHandle,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum CloneStage {
    Idle,
    Fetching,
    Cloning,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct RecentRepo {
    #[serde(rename = "nameWithOwner")]
    pub name_with_owner: String,
    pub description: Option<String>,
}

pub(super) enum ProjectDialogState {
    Sources {
        github_focus: FocusHandle,
        local_focus: FocusHandle,
    },
    Github {
        url: Entity<TextInput>,
        location: Entity<TextInput>,
        open_focus: FocusHandle,
        stage: CloneStage,
        error: Option<String>,
        recent_repos: Vec<RecentRepo>,
        selected_repo: Option<String>,
    },
}

fn default_clone_location() -> PathBuf {
    if let Some(home) = dirs::home_dir() {
        home.join("insulator").join("repos")
    } else {
        PathBuf::from(".")
    }
}

impl Insulator {
    pub(super) fn open_project_source_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let local_focus = cx.focus_handle();
        let github_focus = cx.focus_handle();
        let initial_focus = local_focus.clone();
        self.project_dialog = Some(ProjectDialogState::Sources {
            github_focus,
            local_focus,
        });
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| window.focus(&initial_focus, cx));
        });
        cx.notify();
    }

    pub(super) fn open_github_project_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let url =
            cx.new(|cx| TextInput::new(window, cx).placeholder("https://github.com/user/repo.git"));
        cx.subscribe(&url, |_, _, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Edited) {
                cx.notify();
            }
        })
        .detach();

        let location = cx.new(|cx| TextInput::new(window, cx));
        let default_loc = default_clone_location();
        location.update(cx, |input, cx| {
            input.set_content(default_loc.display().to_string(), cx);
        });

        let focus = url.read(cx).focus();
        self.project_dialog = Some(ProjectDialogState::Github {
            url,
            location,
            open_focus: cx.focus_handle(),
            stage: CloneStage::Idle,
            error: None,
            recent_repos: Vec::new(),
            selected_repo: None,
        });

        // Fetch recent repos in background
        cx.spawn(async move |insulator, cx| {
            let repos = cx
                .background_executor()
                .spawn(async move {
                    let Ok(output) = std::process::Command::new("gh")
                        .args([
                            "repo",
                            "list",
                            "--limit",
                            "15",
                            "--json",
                            "nameWithOwner,description",
                        ])
                        .output()
                    else {
                        return Vec::new();
                    };
                    if !output.status.success() {
                        return Vec::new();
                    }
                    serde_json::from_slice::<Vec<RecentRepo>>(&output.stdout).unwrap_or_default()
                })
                .await;

            let _ = insulator.update(cx, |insulator, cx| {
                if let Some(ProjectDialogState::Github { recent_repos, .. }) =
                    insulator.project_dialog.as_mut()
                {
                    *recent_repos = repos;
                    cx.notify();
                }
            });
        })
        .detach();

        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        });
        cx.notify();
    }

    fn close_project_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.project_dialog.take().is_none() {
            return;
        }
        let focus = self.composer_focus(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    fn choose_local_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_project_dialog(window, cx);
        self.add_project(cx);
    }

    fn choose_clone_location(&mut self, cx: &mut Context<Self>) {
        let Some(ProjectDialogState::Github { location, .. }) = self.project_dialog.as_ref() else {
            return;
        };
        let location = location.clone();
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some(tr!("project.choose_clone_location").into()),
        });
        cx.spawn(async move |_, cx| {
            if let Ok(Ok(Some(paths))) = receiver.await
                && let Some(path) = paths.into_iter().next()
            {
                let _ = location.update(cx, |input, cx| {
                    input.set_content(path.display().to_string(), cx);
                });
            }
        })
        .detach();
    }

    fn clone_github_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(ProjectDialogState::Github {
            url,
            location,
            stage,
            error,
            ..
        }) = self.project_dialog.as_mut()
        else {
            return;
        };
        if *stage != CloneStage::Idle {
            return;
        }
        let repository_url = url.read(cx).content().trim().to_owned();
        let destination = PathBuf::from(location.read(cx).content().trim());
        if repository_url.is_empty() {
            *error = Some(tr!("project.github_url_required"));
            cx.notify();
            return;
        }
        if destination.as_os_str().is_empty() {
            *error = Some(tr!("project.clone_location_required"));
            cx.notify();
            return;
        }
        if !destination.exists() {
            let _ = std::fs::create_dir_all(&destination);
        }

        *stage = CloneStage::Fetching;
        *error = None;
        let workspace = insulator_client::WorkspaceClient::new(self.daemon.client());
        let window_handle = window.window_handle();
        let url_to_clone = repository_url.clone();

        cx.spawn(async move |insulator, cx| {
            // First show "Fetching repository…" briefly to indicate verification phase
            cx.background_executor()
                .timer(Duration::from_millis(400))
                .await;
            let _ = insulator.update(cx, |insulator, cx| {
                if let Some(ProjectDialogState::Github { stage, .. }) =
                    insulator.project_dialog.as_mut()
                {
                    *stage = CloneStage::Cloning;
                    cx.notify();
                }
            });

            let result = cx
                .background_executor()
                .spawn(async move {
                    match workspace.request(insulator_client::WorkspaceOperation::CloneRepository {
                        url: url_to_clone,
                        destination,
                    }) {
                        Ok(insulator_client::WorkspaceResult::ClonedRepository { path }) => {
                            Ok(path)
                        }
                        Ok(_) => Err("the daemon returned an invalid clone response".to_owned()),
                        Err(error) => Err(error.to_string()),
                    }
                })
                .await;

            let focus = insulator.update(cx, |insulator, cx| match result {
                Ok(path) => {
                    insulator.project_dialog = None;
                    insulator.add_project_path(path, cx);
                    Some(insulator.composer_focus(cx))
                }
                Err(message) => {
                    if let Some(ProjectDialogState::Github { stage, error, .. }) =
                        insulator.project_dialog.as_mut()
                    {
                        *stage = CloneStage::Idle;
                        *error = Some(message);
                    }
                    cx.notify();
                    None
                }
            });
            if let Ok(Some(focus)) = focus {
                let _ = window_handle.update(cx, |_, window, cx| window.focus(&focus, cx));
            }
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_project_dialog(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.project_dialog.as_ref()?;
        let theme = Theme::current(cx);
        let weak = cx.entity().downgrade();

        let card = match dialog {
            ProjectDialogState::Sources {
                github_focus,
                local_focus,
            } => {
                let local_click = weak.clone();
                let local_key = weak.clone();
                let github_click = weak.clone();
                let github_key = weak.clone();
                div()
                    .w_full()
                    .max_w(px(420.0))
                    .rounded(px(16.0))
                    .border_1()
                    .border_color(theme.border_strong)
                    .bg(theme.elevated)
                    .shadow_2xl()
                    .p(px(20.0))
                    .font_family(crate::theme::active_ui_font_family())
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(sp(17.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child("New project"),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .mb(px(16.0))
                            .text_size(sp(12.5))
                            .text_color(theme.text_secondary)
                            .child("Open an existing directory or clone a remote Git repository"),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(10.0))
                            .child(
                                project_source_row(
                                    "project-source-local",
                                    local_focus,
                                    "icons/folder-new.svg",
                                    "Open local folder".to_string(),
                                    "Choose an existing directory on your computer".to_string(),
                                    theme,
                                )
                                .on_click(move |_, window, cx| {
                                    let _ = local_click.update(cx, |insulator, cx| {
                                        insulator.choose_local_project(window, cx)
                                    });
                                })
                                .on_key_down(
                                    move |event: &KeyDownEvent, window, cx| {
                                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                        {
                                            let _ = local_key.update(cx, |insulator, cx| {
                                                insulator.choose_local_project(window, cx)
                                            });
                                            cx.stop_propagation();
                                        }
                                    },
                                ),
                            )
                            .child(
                                project_source_row(
                                    "project-source-github",
                                    github_focus,
                                    "icons/github.svg",
                                    "Clone from GitHub".to_string(),
                                    "Clone a repository from GitHub URL".to_string(),
                                    theme,
                                )
                                .on_click(move |_, window, cx| {
                                    let _ = github_click.update(cx, |insulator, cx| {
                                        insulator.open_github_project_dialog(window, cx)
                                    });
                                })
                                .on_key_down(
                                    move |event: &KeyDownEvent, window, cx| {
                                        if matches!(event.keystroke.key.as_str(), "enter" | "space")
                                        {
                                            let _ = github_key.update(cx, |insulator, cx| {
                                                insulator.open_github_project_dialog(window, cx)
                                            });
                                            cx.stop_propagation();
                                        }
                                    },
                                ),
                            ),
                    )
                    .into_any_element()
            }
            ProjectDialogState::Github {
                url,
                location,
                open_focus,
                stage,
                error,
                recent_repos,
                selected_repo,
            } => {
                let is_busy = *stage != CloneStage::Idle;
                let can_open = !is_busy && !url.read(cx).content().trim().is_empty();
                let button_weak = weak.clone();
                let button_key_weak = weak.clone();
                let avatar_path = crate::platform::local_user_github_avatar_path();

                div()
                    .w_full()
                    .max_w(px(520.0))
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(theme.border_strong)
                    .bg(theme.elevated)
                    .shadow_2xl()
                    .p(px(22.0))
                    .font_family(crate::theme::active_ui_font_family())
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(sp(18.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.text)
                            .child(tr!("project.clone_github")),
                    )
                    .child(
                        div()
                            .mt(px(16.0))
                            .text_size(sp(13.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(tr!("project.repository_url")),
                    )
                    .child(
                        TextField::new("github-project-url", url.clone())
                            .mt(px(7.0))
                            .h(px(38.0)),
                    )
                    .child(
                        div()
                            .mt(px(16.0))
                            .text_size(sp(13.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child("Recent repos"),
                    )
                    .child(
                        div()
                            .id("recent-repos-box")
                            .mt(px(7.0))
                            .w_full()
                            .max_h(px(175.0))
                            .overflow_y_scroll()
                            .rounded(px(8.0))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.sidebar_item_background)
                            .children(recent_repos.iter().map(|repo| {
                                let repo_name = repo.name_with_owner.clone();
                                let is_selected = selected_repo.as_ref() == Some(&repo_name);
                                let item_weak = weak.clone();
                                let repo_url = format!("https://github.com/{repo_name}.git");

                                div()
                                    .id(SharedString::from(format!("recent-repo-{repo_name}")))
                                    .w_full()
                                    .px(px(12.0))
                                    .py(px(9.0))
                                    .border_b_1()
                                    .border_color(theme.border)
                                    .flex()
                                    .items_center()
                                    .gap(px(12.0))
                                    .cursor_default()
                                    .when(is_selected, |element| element.bg(theme.overlay_strong))
                                    .hover(|element| element.bg(theme.overlay))
                                    .child(if let Some(avatar) = avatar_path.as_ref() {
                                        img(avatar.clone())
                                            .size(px(28.0))
                                            .rounded_full()
                                            .flex_none()
                                            .into_any_element()
                                    } else {
                                        div()
                                            .size(px(28.0))
                                            .rounded_full()
                                            .bg(theme.accent)
                                            .flex()
                                            .items_center()
                                            .justify_center()
                                            .child(icon("icons/github.svg", 14.0, theme.on_inverse))
                                            .flex_none()
                                            .into_any_element()
                                    })
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .child(
                                                div()
                                                    .text_size(sp(13.5))
                                                    .font_weight(FontWeight::MEDIUM)
                                                    .text_color(theme.text)
                                                    .truncate()
                                                    .child(repo_name.clone()),
                                            )
                                            .when_some(
                                                repo.description.clone().filter(|d| !d.is_empty()),
                                                |parent, desc| {
                                                    parent.child(
                                                        div()
                                                            .mt(px(2.0))
                                                            .text_size(sp(12.0))
                                                            .text_color(theme.text_secondary)
                                                            .truncate()
                                                            .child(desc),
                                                    )
                                                },
                                            ),
                                    )
                                    .on_click(move |_, _, cx| {
                                        let repo_url = repo_url.clone();
                                        let repo_name = repo_name.clone();
                                        let _ = item_weak.update(cx, move |insulator, cx| {
                                            if let Some(ProjectDialogState::Github {
                                                url,
                                                selected_repo,
                                                ..
                                            }) = insulator.project_dialog.as_mut()
                                            {
                                                *selected_repo = Some(repo_name);
                                                url.update(cx, |input, cx| {
                                                    input.set_content(repo_url, cx);
                                                });
                                                cx.notify();
                                            }
                                        });
                                    })
                            }))
                            .when(recent_repos.is_empty(), |box_el| {
                                box_el.p(px(14.0)).child(
                                    div()
                                        .text_size(sp(12.5))
                                        .text_color(theme.text_tertiary)
                                        .child("No recent repositories found from GitHub CLI."),
                                )
                            }),
                    )
                    .child(
                        div()
                            .mt(px(16.0))
                            .text_size(sp(13.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text)
                            .child(tr!("project.clone_location")),
                    )
                    .child(
                        div()
                            .mt(px(7.0))
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                TextField::new("github-project-location", location.clone())
                                    .h(px(38.0))
                                    .flex_1(),
                            )
                            .child(
                                div()
                                    .id("github-project-browse")
                                    .h(px(38.0))
                                    .px(px(16.0))
                                    .rounded(px(7.0))
                                    .border_1()
                                    .border_color(theme.border)
                                    .bg(theme.sidebar_item_background)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_default()
                                    .hover(|button| button.bg(theme.overlay_strong))
                                    .text_size(sp(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(theme.text)
                                    .child(tr!("project.browse"))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.choose_clone_location(cx);
                                    })),
                            ),
                    )
                    .when_some(error.clone(), |card, error| {
                        card.child(
                            div()
                                .mt(px(10.0))
                                .text_size(sp(12.5))
                                .line_height(sp(17.0))
                                .text_color(theme.danger)
                                .child(error),
                        )
                    })
                    .child(
                        div().mt(px(18.0)).flex().justify_end().child(
                            div()
                                .id("github-project-open")
                                .track_focus(open_focus)
                                .when(can_open, |button| button.tab_index(0))
                                .h(px(34.0))
                                .px(px(16.0))
                                .rounded(px(7.0))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .bg(theme.inverse)
                                .text_color(theme.on_inverse)
                                .text_size(sp(13.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .opacity(if can_open { 1.0 } else { 0.45 })
                                .when(can_open, |button| {
                                    button
                                        .hover(|button| button.opacity(0.9))
                                        .on_click(move |_, window, cx| {
                                            let _ = button_weak.update(cx, |insulator, cx| {
                                                insulator.clone_github_project(window, cx)
                                            });
                                        })
                                        .on_key_down(move |event: &KeyDownEvent, window, cx| {
                                            if matches!(
                                                event.keystroke.key.as_str(),
                                                "enter" | "space"
                                            ) {
                                                let _ =
                                                    button_key_weak.update(cx, |insulator, cx| {
                                                        insulator.clone_github_project(window, cx)
                                                    });
                                                cx.stop_propagation();
                                            }
                                        })
                                })
                                .when(is_busy, |button| {
                                    button.child(dot_matrix_loader(theme.on_inverse, 14.0))
                                })
                                .child(match stage {
                                    CloneStage::Idle => tr!("project.clone"),
                                    CloneStage::Fetching => "Fetching repository…".to_string(),
                                    CloneStage::Cloning => "Cloning repository…".to_string(),
                                })
                                .when(!is_busy, |button| {
                                    button.child(
                                        div()
                                            .ml(px(6.0))
                                            .text_size(sp(11.0))
                                            .text_color(theme.on_inverse)
                                            .opacity(0.6)
                                            .child("⌘ ↵"),
                                    )
                                }),
                        ),
                    )
                    .into_any_element()
            }
        };

        let layer = div()
            .id("project-dialog-scrim")
            .key_context(DIALOG_CONTEXT)
            .on_action(
                cx.listener(|insulator, _: &DismissProjectDialog, window, cx| {
                    insulator.close_project_dialog(window, cx)
                }),
            )
            .on_action(
                cx.listener(|insulator, _: &ConfirmGithubProject, window, cx| {
                    insulator.clone_github_project(window, cx)
                }),
            )
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::hsla(0.0, 0.0, 0.0, 0.6))
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.0))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|insulator, _, window, cx| insulator.close_project_dialog(window, cx)),
            )
            .child(
                div()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(card),
            );
        Some(gpui::deferred(layer).with_priority(4).into_any_element())
    }

    pub(super) fn open_rename_project_dialog(
        &mut self,
        project_id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_name = self
            .state
            .projects
            .iter()
            .find(|p| p.id == project_id)
            .map(|p| p.name.clone())
            .unwrap_or_default();
        let input = cx.new(|cx| TextInput::new(window, cx));
        input.update(cx, |input, cx| {
            input.set_content(current_name, cx);
        });
        let focus = input.read(cx).focus();
        self.rename_dialog = Some(RenameDialogState {
            target: RenameTarget::Project(project_id),
            input,
            focus: focus.clone(),
        });
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        });
        cx.notify();
    }

    pub(super) fn open_rename_session_dialog(
        &mut self,
        session_id: Uuid,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current_title = self
            .state
            .sessions
            .iter()
            .find(|s| s.id == session_id)
            .map(sidebar::localized_session_title)
            .unwrap_or_default();
        let input = cx.new(|cx| TextInput::new(window, cx));
        input.update(cx, |input, cx| {
            input.set_content(current_title, cx);
        });
        let focus = input.read(cx).focus();
        self.rename_dialog = Some(RenameDialogState {
            target: RenameTarget::Session(session_id),
            input,
            focus: focus.clone(),
        });
        window.on_next_frame(move |window, _| {
            window.on_next_frame(move |window, cx| window.focus(&focus, cx));
        });
        cx.notify();
    }

    pub(super) fn confirm_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(dialog) = self.rename_dialog.as_ref() else {
            return;
        };
        let new_name = dialog.input.read(cx).content().trim().to_owned();
        if new_name.is_empty() {
            return;
        }
        match dialog.target {
            RenameTarget::Project(project_id) => {
                if let Some(project) = self.state.projects.iter_mut().find(|p| p.id == project_id) {
                    project.name = new_name;
                    self.save();
                }
            }
            RenameTarget::Session(session_id) => {
                if let Some(session) = self.state.sessions.iter_mut().find(|s| s.id == session_id) {
                    session.title = new_name;
                    session.auto_title = None;
                    self.save();
                }
            }
        }
        self.rename_dialog = None;
        let focus = self.composer_focus(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn close_rename_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.rename_dialog.take().is_none() {
            return;
        }
        let focus = self.composer_focus(cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    pub(super) fn render_rename_dialog(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let dialog = self.rename_dialog.as_ref()?;
        let theme = Theme::current(cx);
        let weak = cx.entity().downgrade();

        let (title_text, label_text) = match dialog.target {
            RenameTarget::Project(_) => ("Rename project", "Project name"),
            RenameTarget::Session(_) => ("Rename chat", "Chat title"),
        };
        let input = dialog.input.clone();
        let cancel_weak = weak.clone();
        let confirm_weak = weak.clone();

        let card = div()
            .w_full()
            .max_w(px(420.0))
            .rounded(px(14.0))
            .border_1()
            .border_color(theme.border_strong)
            .bg(theme.elevated)
            .shadow_2xl()
            .p(px(20.0))
            .font_family(crate::theme::active_ui_font_family())
            .flex()
            .flex_col()
            .child(
                div()
                    .text_size(sp(17.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.text)
                    .child(title_text),
            )
            .child(
                div()
                    .mt(px(14.0))
                    .text_size(sp(13.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(theme.text_secondary)
                    .child(label_text),
            )
            .child(
                TextField::new("rename-dialog-input", input)
                    .mt(px(6.0))
                    .h(px(36.0)),
            )
            .child(
                div()
                    .mt(px(18.0))
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(10.0))
                    .child(
                        div()
                            .id("rename-dialog-cancel")
                            .h(px(32.0))
                            .px(px(14.0))
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(theme.border)
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_default()
                            .hover(|btn| btn.bg(theme.overlay))
                            .text_size(sp(12.5))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(theme.text_secondary)
                            .child("Cancel")
                            .on_click(move |_, window, cx| {
                                let _ = cancel_weak.update(cx, |insulator, cx| {
                                    insulator.close_rename_dialog(window, cx);
                                });
                            }),
                    )
                    .child(
                        div()
                            .id("rename-dialog-confirm")
                            .h(px(32.0))
                            .px(px(16.0))
                            .rounded(px(6.0))
                            .bg(theme.inverse)
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_default()
                            .hover(|btn| btn.opacity(0.9))
                            .text_size(sp(12.5))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(theme.on_inverse)
                            .child("Rename")
                            .on_click(move |_, window, cx| {
                                let _ = confirm_weak.update(cx, |insulator, cx| {
                                    insulator.confirm_rename(window, cx);
                                });
                            }),
                    ),
            );

        let layer = div()
            .id("rename-dialog-scrim")
            .key_context(RENAME_DIALOG_CONTEXT)
            .on_action(cx.listener(|insulator, _: &DismissRename, window, cx| {
                insulator.close_rename_dialog(window, cx)
            }))
            .on_action(cx.listener(|insulator, _: &ConfirmRename, window, cx| {
                insulator.confirm_rename(window, cx)
            }))
            .absolute()
            .inset_0()
            .occlude()
            .bg(gpui::hsla(0.0, 0.0, 0.0, 0.6))
            .flex()
            .items_center()
            .justify_center()
            .p(px(24.0))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|insulator, _, window, cx| insulator.close_rename_dialog(window, cx)),
            )
            .child(
                div()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(card),
            );
        Some(gpui::deferred(layer).with_priority(4).into_any_element())
    }
}

fn project_source_row(
    id: &'static str,
    focus: &FocusHandle,
    icon_path: &'static str,
    title: String,
    description: String,
    theme: Theme,
) -> Stateful<Div> {
    div()
        .id(id)
        .track_focus(focus)
        .tab_index(0)
        .w_full()
        .h(px(64.0))
        .px(px(14.0))
        .rounded(px(10.0))
        .border_1()
        .border_color(theme.border)
        .bg(theme.sidebar_item_background)
        .flex()
        .items_center()
        .gap(px(14.0))
        .cursor_default()
        .focus_visible(|style| style.border_1().border_color(theme.accent))
        .hover(|row| row.bg(theme.overlay))
        .active(|row| row.bg(theme.overlay_strong))
        .child(
            div()
                .size(px(38.0))
                .rounded(px(8.0))
                .bg(theme.overlay)
                .flex()
                .items_center()
                .justify_center()
                .child(icon(icon_path, 18.0, theme.text)),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .child(
                    div()
                        .text_size(sp(14.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(theme.text)
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(sp(12.0))
                        .text_color(theme.text_secondary)
                        .child(description),
                ),
        )
        .child(icon("icons/chevron-right.svg", 14.0, theme.text_tertiary))
}
