use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

use anyhow::Result;
use gpui::{App, AssetSource, SharedString};

/// Icons embedded in the binary so the app stays a single artifact.
pub struct Assets;

macro_rules! icons {
    ($($name:literal),+ $(,)?) => {
        &[$((
            concat!("icons/", $name, ".svg"),
            include_bytes!(concat!("../assets/icons/", $name, ".svg")).as_slice(),
        )),+]
    };
}

const ICONS: &[(&str, &[u8])] = icons![
    "alert",
    "appearance",
    "arrow-down",
    "arrow-left",
    "arrow-right",
    "arrow-up",
    "arrow-up-right",
    "block",
    "bot",
    "case-sensitive",
    "chart-column",
    "check",
    "circle-dashed",
    "changes",
    "cloud-upload",
    "chevron-down",
    "chevron-right",
    "chevron-up",
    "chevrons-up-down",
    "command",
    "compose",
    "copy",
    "corner-down-right",
    "cpu",
    "cursor-spark",
    "download",
    "ellipsis",
    "eye",
    "eye-off",
    "external-link",
    "file",
    "folder",
    "folder-outline",
    "folder-plus",
    "home",
    "folder-new",
    "folder-open",
    "file-bottom-left-arrow",
    "file-diff",
    "file-types/angular",
    "file-types/audio",
    "file-types/astro",
    "file-types/babel",
    "file-types/biome",
    "file-types/bun",
    "file-types/c",
    "file-types/certificate",
    "file-types/clojure",
    "file-types/cmake",
    "file-types/coffee",
    "file-types/console",
    "file-types/cpp",
    "file-types/crystal",
    "file-types/csharp",
    "file-types/css",
    "file-types/dart",
    "file-types/database",
    "file-types/deno",
    "file-types/diff",
    "file-types/docker",
    "file-types/editorconfig",
    "file-types/elixir",
    "file-types/elm",
    "file-types/erlang",
    "file-types/eslint",
    "file-types/exe",
    "file-types/file",
    "file-types/firebase",
    "file-types/git",
    "file-types/gitlab",
    "file-types/go",
    "file-types/gradle",
    "file-types/graphql",
    "file-types/haskell",
    "file-types/haxe",
    "file-types/helm",
    "file-types/html",
    "file-types/image",
    "file-types/java",
    "file-types/javascript",
    "file-types/jinja",
    "file-types/json",
    "file-types/julia",
    "file-types/kotlin",
    "file-types/kubernetes",
    "file-types/lock",
    "file-types/lua",
    "file-types/makefile",
    "file-types/markdown",
    "file-types/nest",
    "file-types/next",
    "file-types/nginx",
    "file-types/nix",
    "file-types/nodejs",
    "file-types/npm",
    "file-types/nuxt",
    "file-types/ocaml",
    "file-types/pdf",
    "file-types/perl",
    "file-types/php",
    "file-types/pnpm",
    "file-types/powershell",
    "file-types/prettier",
    "file-types/prisma",
    "file-types/proto",
    "file-types/pug",
    "file-types/python",
    "file-types/react",
    "file-types/readme",
    "file-types/rollup",
    "file-types/ruby",
    "file-types/rust",
    "file-types/sass",
    "file-types/scala",
    "file-types/settings",
    "file-types/solidity",
    "file-types/storybook",
    "file-types/stylelint",
    "file-types/supabase",
    "file-types/svelte",
    "file-types/svg",
    "file-types/swift",
    "file-types/tailwindcss",
    "file-types/terraform",
    "file-types/tex",
    "file-types/turborepo",
    "file-types/typescript",
    "file-types/video",
    "file-types/vite",
    "file-types/vitest",
    "file-types/vue",
    "file-types/webassembly",
    "file-types/webpack",
    "file-types/xaml",
    "file-types/xml",
    "file-types/yaml",
    "file-types/yarn",
    "file-types/zig",
    "file-types/zip",
    "fork",
    "git-branch",
    "git-commit-horizontal",
    "git-pull-request",
    "globe",
    "github",
    "hourglass",
    "hexagon",
    "history",
    "info",
    "laptop",
    "layers",
    "list",
    "list-filter",
    "loader-circle",
    "lock",
    "lock-open",
    "message-square",
    "minus",
    "package",
    "panel-left",
    "panel-right",
    "pencil",
    "plus",
    "new-task",
    "provider-amp",
    "provider-claude",
    "provider-cursor",
    "provider-deepseek",
    "provider-devin",
    "provider-fx",
    "provider-grok",
    "provider-kimi",
    "provider-openai",
    "provider-ohmypi",
    "provider-opencode",
    "provider-opencode2",
    "provider-opencode2-badge",
    "provider-pi",
    "queue",
    "regex",
    "replace",
    "replace-all",
    "rewind",
    "rotate-cw",
    "search",
    "server",
    "settings",
    "slash",
    "sparkle",
    "star",
    "star-filled",
    "status-backlog",
    "status-canceled",
    "status-done",
    "status-in-progress",
    "status-in-review",
    "stop",
    "stop-filled",
    "target",
    "terminal",
    "terminal-square",
    "trash",
    "users",
    "whole-word",
    "wrench",
    "window-maximize",
    "window-minimize",
    "window-restore",
    "x",
    "zap",
];

const TEXT_FONTS: &[&[u8]] = &[
    include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Bold.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-Italic.ttf"),
    include_bytes!("../assets/fonts/JetBrainsMono-BoldItalic.ttf"),
];

/// Symbols-only icon face resolved via CoreText cascade (`FontFallbacks`),
/// never as a primary GPUI family; see `register_fonts_with_coretext`.
const SYMBOLS_FONT: &[u8] = include_bytes!("../assets/fonts/SymbolsNerdFontMono-Regular.ttf");
static HOME_LOGO: OnceLock<Arc<gpui::Image>> = OnceLock::new();
static LIGHT_HOME_LOGO: OnceLock<Arc<gpui::Image>> = OnceLock::new();
static INSULATOR_MARK: OnceLock<Arc<gpui::Image>> = OnceLock::new();

/// Family name of [`SYMBOLS_FONT`] for `FontFallbacks` lists.
pub const SYMBOLS_FONT_FAMILY: &str = "Symbols Nerd Font Mono";

pub fn home_logo() -> Arc<gpui::Image> {
    HOME_LOGO
        .get_or_init(|| {
            Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                include_bytes!("../assets/insulator-logo.png").to_vec(),
            ))
        })
        .clone()
}

pub fn light_home_logo() -> Arc<gpui::Image> {
    LIGHT_HOME_LOGO
        .get_or_init(|| {
            Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                include_bytes!("../assets/insulator-logo-light.png").to_vec(),
            ))
        })
        .clone()
}

pub fn insulator_mark() -> Arc<gpui::Image> {
    INSULATOR_MARK
        .get_or_init(|| {
            Arc::new(gpui::Image::from_bytes(
                gpui::ImageFormat::Png,
                include_bytes!("../assets/insulator-mark.png").to_vec(),
            ))
        })
        .clone()
}

pub fn register_fonts(cx: &App) -> Result<()> {
    cx.text_system().add_fonts(
        TEXT_FONTS
            .iter()
            .map(|font| Cow::Borrowed(*font))
            .collect::<Vec<_>>(),
    )?;
    crate::platform::register_fonts_with_coretext(&[SYMBOLS_FONT])
}

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(ICONS
            .iter()
            .find(|(name, _)| *name == path)
            .map(|(_, bytes)| Cow::Borrowed(*bytes)))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(ICONS
            .iter()
            .filter(|(name, _)| name.starts_with(path))
            .map(|(name, _)| SharedString::from(*name))
            .collect())
    }
}
