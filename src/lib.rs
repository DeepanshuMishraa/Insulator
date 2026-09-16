#![recursion_limit = "256"]

rust_i18n::i18n!("locales", fallback = "en");

// rust-i18n expands locale data in a proc macro, which Cargo does not always
// discover as an input when only a YAML file changes. Keep explicit source
// dependencies so the watcher rebuilds the translation registry itself.
const _LOCALE_SOURCES: [&str; 3] = [
    include_str!("../locales/app.yml"),
    include_str!("../locales/zh-CN.yml"),
    include_str!("../locales/ja.yml"),
];

macro_rules! tr {
    ($key:expr) => {
        crate::i18n::translate($key)
    };
    ($key:expr, $($args:tt)*) => {
        rust_i18n::t!($key, $($args)*).into_owned()
    };
}

/// Borrow static translations on hot render paths; interpolation uses `tr!`
/// because formatted messages necessarily allocate.
macro_rules! tr_cow {
    ($key:literal) => {
        rust_i18n::t!($key)
    };
}

mod app;
mod assets;
mod audio;
mod browser;
mod computer_use;
pub mod daemon;
mod driver;
pub mod haptics;
mod input;
mod md;
mod platform;
mod query;
mod review_diff;
mod terminal;
mod theme;
mod ui;
mod updater;

pub use insulator_client::{
    checkpoint, command_env, composer_complete, git_branch, git_commit, i18n, identity, model,
    model_catalog, persistence, projectless, skills, usage, usage_history, worktree,
};

use gpui::{
    App, Application, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions,
    WindowBackgroundAppearance, WindowBounds, WindowOptions, actions, point, px, size,
};

use crate::app::Insulator;
use crate::identity::{APP_ID, APP_NAME};
actions!(
    insulator,
    [
        Quit,
        About,
        CloseWindow,
        NewSession,
        NewTab,
        NewProject,
        OpenSettings,
        CheckForUpdates,
        ToggleSidebar,
        ToggleRightPanel,
        ToggleCommandPalette,
        OpenResumePicker,
        ToggleFpsCounter,
        NavigateBack,
        NavigateForward,
        SwitchTaskForward,
        SwitchTaskBackward,
        SelectFirstTask,
        SelectLastTask,
        ConfirmTaskSwitch,
        CancelTaskSwitch,
        FocusComposer,
        ToggleModelPicker,
        ToggleUsagePanel,
        OpenReview,
        TogglePullRequests,
        NextMainTab,
        PreviousMainTab,
        CloseActiveTab,
        SaveFile,
        CancelTurn,
        CopySelection,
        OpenFind,
        OpenFindReplace,
        CloseFind,
        FindNext,
        FindPrevious,
        ToggleFindCaseSensitive,
        ToggleFindWholeWord,
        ToggleFindRegex,
        ReplaceAllMatches,
        BrowserBack,
        BrowserForward,
        BrowserReload,
        BrowserHardReload,
        BrowserStop,
        BrowserDevtools,
        FocusBrowserAddress,
        BrowserAddressCancel,
        WebviewCopy,
        WebviewCut,
        WebviewPaste,
        WebviewSelectAll
    ]
);

const DEFAULT_WINDOW_WIDTH: f32 = 1380.0;
const DEFAULT_WINDOW_HEIGHT: f32 = 880.0;
const MIN_WINDOW_WIDTH: f32 = 980.0;
const MIN_WINDOW_HEIGHT: f32 = 680.0;
/// How much titlebar must stay on the display for the window to be dragged
/// back by hand.
const TITLEBAR_GRAB_WIDTH: f32 = 160.0;
const TITLEBAR_GRAB_HEIGHT: f32 = 22.0;

/// Reopen the main window where the user last left it, on the display it was
/// left on. GPUI window bounds are display-relative, so the persisted frame is
/// anchored by resolving the saved display UUID against the connected
/// displays — Zed's scheme — and `display_id` rides along in `WindowOptions`.
/// When that display is gone the same offsets re-anchor on the primary
/// display, and the origin is clamped so the titlebar stays grabbable after
/// any display change.
fn restored_window_placement(cx: &App) -> (WindowBounds, Option<gpui::DisplayId>) {
    let centered = |cx: &App| {
        (
            WindowBounds::Windowed(Bounds::centered(
                None,
                size(px(DEFAULT_WINDOW_WIDTH), px(DEFAULT_WINDOW_HEIGHT)),
                cx,
            )),
            None,
        )
    };
    let Some(saved) = crate::persistence::load_window_state().filter(|saved| {
        [saved.x, saved.y, saved.width, saved.height]
            .iter()
            .all(|value| value.is_finite())
    }) else {
        return centered(cx);
    };
    let display = saved.display.and_then(|uuid| {
        cx.displays()
            .into_iter()
            .find(|display| display.uuid().ok() == Some(uuid))
    });
    let display_id = display.as_ref().map(|display| display.id());
    let Some(anchor) = display.or_else(|| cx.primary_display()) else {
        return centered(cx);
    };
    let anchor_size = anchor.bounds().size;
    let width = saved.width.max(MIN_WINDOW_WIDTH);
    let height = saved.height.max(MIN_WINDOW_HEIGHT);
    let x = saved.x.clamp(
        TITLEBAR_GRAB_WIDTH - width,
        (f32::from(anchor_size.width) - TITLEBAR_GRAB_WIDTH).max(0.0),
    );
    let y = saved.y.clamp(
        0.0,
        (f32::from(anchor_size.height) - TITLEBAR_GRAB_HEIGHT).max(0.0),
    );
    let bounds = Bounds::new(point(px(x), px(y)), size(px(width), px(height)));
    let window_bounds = if saved.maximized {
        WindowBounds::Maximized(bounds)
    } else {
        WindowBounds::Windowed(bounds)
    };
    (window_bounds, display_id)
}

trait InsulatorApplicationExt {
    fn with_main_window_reopen(self) -> Self;
}

impl InsulatorApplicationExt for Application {
    fn with_main_window_reopen(self) -> Self {
        self.on_reopen(|cx| {
            if let Some(window) = cx.windows().into_iter().next() {
                window
                    .update(cx, |_, window, _| window.activate_window())
                    .ok();
            }
            cx.activate(true);
        });
        self
    }
}

pub fn run() {
    let daemon = crate::daemon::start_process()
        .unwrap_or_else(|error| panic!("failed to start Insulator daemon: {error:#}"));
    gpui_platform::application()
        .with_assets(crate::assets::Assets)
        .with_main_window_reopen()
        .run(move |cx: &mut App| {
            // Linux uses this for Wayland app_id/X11 WM_CLASS and notification
            // attribution. Other platforms also benefit from one stable
            // process identity.
            cx.set_app_identity(APP_ID, APP_NAME);
            crate::assets::register_fonts(cx).expect("failed to register bundled fonts");
            crate::input::init(cx);
            crate::ui::menu::init(cx);
            crate::app::init_composer_autocomplete(cx);
            crate::app::init_settings_keys(cx);
            crate::app::init_command_palette(cx);
            crate::app::init_commit_dialog_keys(cx);
            crate::app::init_goal_dialog_keys(cx);
            crate::app::init_image_preview_keys(cx);
            crate::app::init_project_dialog_keys(cx);
            crate::app::init_sidebar_keys(cx);
            crate::app::init_skills_keys(cx);
            crate::theme::init(cx);
            crate::platform::init_reduce_motion(cx);

            // Platform updaters only run from a supported release layout (or
            // when explicitly forced for development); everywhere else the
            // menu item is omitted along with the updater itself.
            let updater = crate::updater::Updater::init();
            let updater_available = updater.is_some();
            cx.set_global(crate::updater::UpdaterState(updater));
            cx.on_action(|_: &CheckForUpdates, cx| {
                if let Some(updater) = &cx.global::<crate::updater::UpdaterState>().0 {
                    updater.check_for_updates();
                }
            });
            cx.on_action(|_: &About, _| crate::platform::show_about_panel());

            // User-customized shortcuts persist in app settings. The main
            // keymap is built from them so an override replaces its default
            // (or unbinds it with "") instead of stacking beside it.
            let key_overrides = crate::persistence::load_or_create_app_settings()
                .map(|settings| settings.keybindings)
                .unwrap_or_default();
            bind_main_keys(cx, &key_overrides);

            cx.on_action(|_: &Quit, cx| cx.quit());

            // Unlike AppKit, Linux has no Dock activation path that can
            // restore a hidden last window. Follow Zed's GPUI precedent and
            // terminate when the final window closes.
            #[cfg(not(target_os = "macos"))]
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            let (window_bounds, display_id) = restored_window_placement(cx);
            let window = cx
                .open_window(
                    WindowOptions {
                        titlebar: Some(TitlebarOptions {
                            title: Some(APP_NAME.into()),
                            // Windows creates the window without `WS_CAPTION`
                            // either way; asking for the transparent titlebar
                            // is what extends the client area over the frame
                            // so Insulator's own header can host the caption
                            // buttons and drag region.
                            appears_transparent: cfg!(any(
                                target_os = "macos",
                                target_os = "windows"
                            )),
                            traffic_light_position: cfg!(target_os = "macos")
                                .then(|| point(px(16.0), px(17.0))),
                        }),
                        // Insulator moves its custom macOS titlebar explicitly. Keep
                        // the NSWindow movable so native controls and Window-menu
                        // tiling remain enabled.
                        is_movable: true,
                        app_owns_titlebar_drag: cfg!(target_os = "macos"),
                        window_background: if cfg!(target_os = "macos") {
                            WindowBackgroundAppearance::Blurred
                        } else {
                            WindowBackgroundAppearance::Opaque
                        },
                        app_id: Some(APP_ID.to_owned()),
                        // GPUI defaults to compositor/server decorations. If a
                        // Wayland compositor declines them, it reports the
                        // client fallback and Insulator renders that frame itself.
                        #[cfg(target_os = "linux")]
                        icon: crate::platform::linux_app_icon(),
                        window_bounds: Some(window_bounds),
                        display_id,
                        window_min_size: Some(size(px(MIN_WINDOW_WIDTH), px(MIN_WINDOW_HEIGHT))),
                        ..Default::default()
                    },
                    move |window, cx| {
                        crate::platform::configure_main_window_close_behavior(window, cx);
                        let insulator = Insulator::new(window, cx, daemon);
                        let composer_focus = insulator.read(cx).composer_focus(cx);
                        window.focus(&composer_focus, cx);
                        insulator
                    },
                )
                .expect("failed to open Insulator window");

            let window_handle = window;
            let async_cx = cx.to_async();
            let executor = cx.foreground_executor().clone();
            crate::platform::register_tab_cycle_handler(move |reverse| {
                let mut cx = async_cx.clone();
                let window_handle = window_handle;
                executor
                    .spawn(async move {
                        let _ = window_handle.update(&mut cx, |insulator, window, cx| {
                            insulator.cycle_task_switcher(reverse, window, cx);
                        });
                    })
                    .detach();
            });

            cx.on_system_notification_response({
                let window = window;
                move |response, cx| {
                    let Some(session_id) = crate::app::task_id_from_notification_tag(&response.tag)
                    else {
                        return;
                    };
                    window
                        .update(cx, |insulator, window, cx| {
                            insulator.open_task_from_notification(session_id, cx);
                            window.activate_window();
                            cx.activate(true);
                        })
                        .ok();
                    cx.dismiss_system_notification(&response.tag);
                }
            });

            window
                .update(cx, |insulator, window, cx| {
                    let theme = crate::theme::Theme::current(cx);
                    let (window_style, color_theme, background_image_path, sidebar_transparency) =
                        insulator.window_style_config();
                    crate::platform::configure_window_style(
                        window,
                        window_style,
                        color_theme,
                        background_image_path,
                    );
                    crate::platform::configure_sidebar_material(
                        window,
                        theme.is_dark,
                        theme.sidebar_drag_background,
                        sidebar_transparency,
                    );
                    cx.activate(true);
                })
                .ok();

            set_app_menus(cx, updater_available);
            // A Linux handoff retains the previous prefix until this freshly
            // relaunched build has successfully opened its main window.
            crate::updater::signal_relaunch_ready();
        });
}

/// Build the window keymap, substituting user overrides from settings.
/// An override replaces its default chord; `""` unbinds the action.
fn bind_main_keys(cx: &mut App, overrides: &std::collections::HashMap<String, String>) {
    /// Effective chord for `id`: override when present, else `default`.
    /// `None` means unbound. A custom chord that fails to parse (hand-edited
    /// garbage) falls back to the default instead of panicking the keymap.
    fn eff(
        overrides: &std::collections::HashMap<String, String>,
        id: &str,
        default: &str,
    ) -> Option<String> {
        match overrides.get(id) {
            Some(custom) if custom.is_empty() => None,
            Some(custom) if gpui::Keystroke::parse(custom).is_ok() => Some(custom.clone()),
            Some(_) | None => Some(default.to_owned()),
        }
    }
    let mut bindings: Vec<KeyBinding> = Vec::new();
    // Fixed bindings (not customizable).
    bindings.extend([
        // `secondary` is Command on macOS and Control elsewhere.
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-alt-shift-f", ToggleFpsCounter, None),
        KeyBinding::new("ctrl-escape", CancelTaskSwitch, Some("Insulator")),
        KeyBinding::new("ctrl-shift-escape", CancelTaskSwitch, Some("Insulator")),
        KeyBinding::new("down", SwitchTaskForward, Some("TaskSwitcher")),
        KeyBinding::new("right", SwitchTaskForward, Some("TaskSwitcher")),
        KeyBinding::new("up", SwitchTaskBackward, Some("TaskSwitcher")),
        KeyBinding::new("left", SwitchTaskBackward, Some("TaskSwitcher")),
        KeyBinding::new("home", SelectFirstTask, Some("TaskSwitcher")),
        KeyBinding::new("end", SelectLastTask, Some("TaskSwitcher")),
        KeyBinding::new("enter", ConfirmTaskSwitch, Some("TaskSwitcher")),
        KeyBinding::new("escape", CancelTaskSwitch, Some("TaskSwitcher")),
        KeyBinding::new("escape", CancelTurn, Some("Insulator")),
        KeyBinding::new("secondary-c", CopySelection, Some("Insulator")),
        KeyBinding::new("secondary-alt-f", OpenFindReplace, Some("Insulator")),
        KeyBinding::new("escape", CloseFind, Some("FileEditorPane")),
        KeyBinding::new("escape", CloseFind, Some("FindBar")),
        KeyBinding::new(
            "secondary-alt-c",
            ToggleFindCaseSensitive,
            Some("FileEditorPane"),
        ),
        KeyBinding::new(
            "secondary-alt-w",
            ToggleFindWholeWord,
            Some("FileEditorPane"),
        ),
        KeyBinding::new("secondary-alt-r", ToggleFindRegex, Some("FileEditorPane")),
        KeyBinding::new("shift-enter", FindPrevious, Some("FindBar")),
        KeyBinding::new("secondary-alt-enter", ReplaceAllMatches, Some("FindBar")),
        // Browser surface. Deeper than "Insulator", so while focus is on the
        // page or its address bar the browser reads the platform's
        // conventional navigation shortcuts; the same keys elsewhere
        // keep their app meanings. The clipboard trio is rebound
        // because GPUI's window view claims key equivalents before
        // AppKit can walk the responder chain into the webview.
        KeyBinding::new("secondary-l", FocusBrowserAddress, Some("Browser")),
        KeyBinding::new("secondary-r", BrowserReload, Some("Browser")),
        KeyBinding::new("secondary-shift-r", BrowserHardReload, Some("Browser")),
        KeyBinding::new("secondary-[", BrowserBack, Some("Browser")),
        KeyBinding::new("secondary-]", BrowserForward, Some("Browser")),
        KeyBinding::new("escape", BrowserStop, Some("Browser")),
        KeyBinding::new("secondary-alt-i", BrowserDevtools, Some("Browser")),
        KeyBinding::new("secondary-c", WebviewCopy, Some("Browser")),
        KeyBinding::new("secondary-x", WebviewCut, Some("Browser")),
        KeyBinding::new("secondary-v", WebviewPaste, Some("Browser")),
        KeyBinding::new("secondary-a", WebviewSelectAll, Some("Browser")),
        KeyBinding::new("escape", BrowserAddressCancel, Some("BrowserAddress")),
    ]);
    // Customizable single-chord actions.
    macro_rules! single {
        ($id:expr, $default:expr, $action:expr, $ctx:expr) => {
            if let Some(chord) = eff(overrides, $id, $default) {
                bindings.push(KeyBinding::new(chord.as_str(), $action, $ctx));
            }
        };
    }
    single!("close_tab", "secondary-w", CloseWindow, None);
    single!("new_chat", "secondary-n", NewSession, None);
    single!("new_tab", "secondary-t", NewTab, None);
    single!("new_project", "secondary-o", NewProject, None);
    single!("open_settings", "secondary-,", OpenSettings, None);
    single!("toggle_sidebar", "secondary-b", ToggleSidebar, None);
    single!(
        "toggle_right_panel",
        "secondary-shift-b",
        ToggleRightPanel,
        None
    );
    single!("command_palette", "secondary-k", ToggleCommandPalette, None);
    single!("focus_composer", "secondary-l", FocusComposer, None);
    single!("model_picker", "secondary-/", ToggleModelPicker, None);
    single!("usage_panel", "secondary-u", ToggleUsagePanel, None);
    single!(
        "navigate_back",
        "secondary-[",
        NavigateBack,
        Some("Insulator")
    );
    single!("open_review", "secondary-shift-e", OpenReview, None);
    single!(
        "pull_requests",
        "secondary-shift-p",
        TogglePullRequests,
        None
    );
    single!(
        "close_active_tab",
        "secondary-shift-w",
        CloseActiveTab,
        None
    );
    // Navigate-forward and main-tab cycling ship unbound; they bind only
    // when the user assigns a chord.
    for (id, ctx) in [("navigate_forward", Some("Insulator"))] {
        if let Some(custom) = overrides
            .get(id)
            .filter(|chord| !chord.is_empty() && gpui::Keystroke::parse(chord).is_ok())
        {
            bindings.push(KeyBinding::new(custom.as_str(), NavigateForward, ctx));
        }
    }
    if let Some(custom) = overrides
        .get("next_main_tab")
        .filter(|chord| !chord.is_empty() && gpui::Keystroke::parse(chord).is_ok())
    {
        bindings.push(KeyBinding::new(custom.as_str(), NextMainTab, None));
    }
    if let Some(custom) = overrides
        .get("prev_main_tab")
        .filter(|chord| !chord.is_empty() && gpui::Keystroke::parse(chord).is_ok())
    {
        bindings.push(KeyBinding::new(custom.as_str(), PreviousMainTab, None));
    }
    // Multi-context / multi-chord actions: one override replaces every
    // default chord so the old shortcut stops firing.
    match eff(overrides, "save_file", "secondary-s") {
        Some(chord) => {
            for ctx in [
                None,
                Some("FileEditorPane"),
                Some("FileEditorPane > TextInput"),
            ] {
                bindings.push(KeyBinding::new(chord.as_str(), SaveFile, ctx));
            }
        }
        None => {}
    }
    match eff(overrides, "open_find", "secondary-f") {
        Some(chord) => {
            for ctx in [Some("Insulator"), Some("Insulator > TextInput")] {
                bindings.push(KeyBinding::new(chord.as_str(), OpenFind, ctx));
            }
        }
        None => {}
    }
    // The text input's macOS-style Ctrl-F caret binding is more
    // specific than Insulator's root context; the two-context bind above
    // already reasserts the platform primary shortcut for inputs inside
    // this window, so no extra guard is needed here.
    match eff(overrides, "find_next", "secondary-g") {
        Some(chord) => bindings.push(KeyBinding::new(chord.as_str(), FindNext, Some("Insulator"))),
        None => {}
    }
    match eff(overrides, "find_prev", "secondary-shift-g") {
        Some(chord) => {
            bindings.push(KeyBinding::new(
                chord.as_str(),
                FindPrevious,
                Some("Insulator"),
            ));
        }
        None => {}
    }
    // Task (tab) switching ships with two alternates; a custom chord
    // replaces both.
    match overrides.get("next_tab") {
        Some(custom) if custom.is_empty() => {}
        Some(custom) => bindings.push(KeyBinding::new(custom.as_str(), SwitchTaskForward, None)),
        None => {
            bindings.push(KeyBinding::new("ctrl-tab", SwitchTaskForward, None));
            bindings.push(KeyBinding::new(
                "secondary-shift-]",
                SwitchTaskForward,
                None,
            ));
        }
    }
    match overrides.get("prev_tab") {
        Some(custom) if custom.is_empty() => {}
        Some(custom) => bindings.push(KeyBinding::new(custom.as_str(), SwitchTaskBackward, None)),
        None => {
            bindings.push(KeyBinding::new("ctrl-shift-tab", SwitchTaskBackward, None));
            bindings.push(KeyBinding::new(
                "secondary-shift-[",
                SwitchTaskBackward,
                None,
            ));
        }
    }
    // Scoped to the editor pane: escape closes the bar there and
    // falls through to CancelTurn anywhere else. (Fixed.)
    cx.bind_keys(bindings);
}

/// Rebuild the whole keymap from the given overrides. Settings calls this
/// after every keybinding change so the live keymap always equals a fresh
/// startup keymap: no stale chords survive a rebind, an unbind, or a reset.
pub fn rebind_all_keys(cx: &mut App, overrides: &std::collections::HashMap<String, String>) {
    cx.clear_key_bindings();
    crate::input::init(cx);
    crate::ui::menu::init(cx);
    crate::app::init_composer_autocomplete(cx);
    crate::app::init_settings_keys(cx);
    crate::app::init_command_palette(cx);
    crate::app::init_commit_dialog_keys(cx);
    crate::app::init_goal_dialog_keys(cx);
    crate::app::init_image_preview_keys(cx);
    crate::app::init_project_dialog_keys(cx);
    crate::app::init_sidebar_keys(cx);
    crate::app::init_skills_keys(cx);
    bind_main_keys(cx, overrides);
}

/// Rebuild the native menu bar in the active locale. GPUI menus own their
/// labels, so changing language must replace the model as well as redraw the
/// window.
pub(crate) fn set_app_menus(cx: &mut App, updater_available: bool) {
    cx.set_menus(vec![
        Menu {
            name: APP_NAME.into(),
            disabled: false,
            items: {
                let mut items = vec![MenuItem::action(tr!("menu.about", app = APP_NAME), About)];
                if updater_available {
                    items.push(MenuItem::action(
                        tr!("menu.check_for_updates"),
                        CheckForUpdates,
                    ));
                }
                items.push(MenuItem::separator());
                items.extend([
                    MenuItem::action(tr!("menu.settings"), OpenSettings),
                    MenuItem::separator(),
                    MenuItem::action(tr!("menu.quit", app = APP_NAME), Quit),
                ]);
                items
            },
        },
        Menu {
            name: tr!("menu.file").into(),
            disabled: false,
            items: vec![
                MenuItem::action(tr!("menu.new_task"), NewSession),
                MenuItem::action(tr!("menu.new_project"), NewProject),
            ],
        },
        Menu {
            name: tr!("menu.edit").into(),
            disabled: false,
            items: vec![
                MenuItem::action(tr!("menu.undo"), input::Undo),
                MenuItem::action(tr!("menu.redo"), input::Redo),
                MenuItem::separator(),
                MenuItem::action(tr!("menu.cut"), input::Cut),
                MenuItem::action(tr!("menu.copy"), input::Copy),
                MenuItem::action(tr!("menu.paste"), input::Paste),
                MenuItem::action(tr!("menu.select_all"), input::SelectAll),
            ],
        },
        Menu {
            name: tr!("menu.view").into(),
            disabled: false,
            items: vec![
                MenuItem::action(tr!("menu.command_palette"), ToggleCommandPalette),
                MenuItem::separator(),
                MenuItem::action(tr!("menu.toggle_sidebar"), ToggleSidebar),
                MenuItem::action(tr!("menu.toggle_right_panel"), ToggleRightPanel),
                MenuItem::action(tr!("menu.focus_composer"), FocusComposer),
                MenuItem::action(tr!("menu.toggle_model_picker"), ToggleModelPicker),
                MenuItem::action(tr!("menu.toggle_usage_panel"), ToggleUsagePanel),
            ],
        },
        Menu {
            name: tr!("menu.window").into(),
            disabled: false,
            items: vec![
                MenuItem::action(tr!("menu.toggle_fps_counter"), ToggleFpsCounter),
                MenuItem::action(tr!("menu.close_window"), CloseWindow),
            ],
        },
    ]);
}
