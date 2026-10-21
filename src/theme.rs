use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{OnceLock, RwLock};

use gpui::{
    App, Context, Global, Hsla, Pixels, Rems, Window, WindowAppearance, hsla, px, rems, rgb,
    transparent_black,
};

pub use insulator_client::theme::{ColorTheme, ThemePreference};
pub use insulator_protocol::theme::{AccentColor, WindowStyle};

static ACTIVE_UI_FONT_FAMILY: OnceLock<RwLock<&'static str>> = OnceLock::new();

pub fn active_ui_font_family() -> &'static str {
    *ACTIVE_UI_FONT_FAMILY
        .get_or_init(|| RwLock::new(insulator_client::persistence::DEFAULT_UI_FONT_FAMILY))
        .read()
        .expect("active UI font lock poisoned")
}

pub fn set_active_ui_font_family(family: String) {
    let family: &'static str = Box::leak(family.into_boxed_str());
    *ACTIVE_UI_FONT_FAMILY
        .get_or_init(|| RwLock::new(insulator_client::persistence::DEFAULT_UI_FONT_FAMILY))
        .write()
        .expect("active UI font lock poisoned") = family;
}

/// Scaled pixels: a dimension authored at the default 14px UI font size,
/// expressed in rems so the UI font size setting scales it. The window's rem
/// size *is* the UI font size, so at the default setting this resolves to
/// exactly the authored pixel value.
///
/// Chrome text sizes and their line heights go through here. Content surfaces
/// that already derive from a font-size setting — markdown metrics, the file
/// editor, diff rows, tool-output mono — stay in `px` so they never scale
/// twice.
pub fn sp(value: f32) -> Rems {
    rems(value / insulator_client::persistence::DEFAULT_UI_FONT_SIZE)
}

/// Corner rounding as a multiple of each element's authored radius, stored as
/// `f32` bits. 0 draws square corners, 1 the authored design, 2 the most
/// rounded; GPUI clamps a radius to half the element, so 2 never overdraws.
static CORNER_SCALE: AtomicU32 = AtomicU32::new(0);

/// Largest multiple of the authored radius the rounding setting reaches.
pub const MAX_CORNER_SCALE: f32 = 2.0;

pub fn corner_scale() -> f32 {
    f32::from_bits(CORNER_SCALE.load(Ordering::Relaxed))
}

/// Set from the rounding setting, a percentage of [`MAX_CORNER_SCALE`].
pub fn set_corner_rounding(percent: f32) {
    let scale = percent.clamp(0.0, 100.0) / 100.0 * MAX_CORNER_SCALE;
    CORNER_SCALE.store(scale.to_bits(), Ordering::Relaxed);
}

/// A corner radius authored at the design's default rounding, scaled by the
/// user's rounding setting.
pub fn radius(value: f32) -> Pixels {
    px(value * corner_scale())
}

/// Liquid Glass see-through, 0 to 100, stored as `f32` bits. 50 is the
/// authored look.
static GLASS_LEVEL: AtomicU32 = AtomicU32::new(0x4248_0000); // 50.0

pub fn set_glass_level(level: f32) {
    GLASS_LEVEL.store(
        insulator_client::persistence::sanitized_glass_level(level).to_bits(),
        Ordering::Relaxed,
    );
}

/// Multiplier for Liquid Glass tint and surface opacity: 2 at level 0, 1 at
/// the default 50, 0 at 100. Callers clamp the product to 1.
pub fn glass_opacity_factor() -> f32 {
    let level = f32::from_bits(GLASS_LEVEL.load(Ordering::Relaxed));
    (1.0 - level / 100.0) * 2.0
}

/// Opacity of the native glass tint behind the window.
pub fn liquid_glass_tint_alpha(dark: bool) -> f32 {
    let base = if dark { 0.45 } else { 0.35 };
    (base * glass_opacity_factor()).min(1.0)
}

fn resolves_to_dark(preference: ThemePreference, system_appearance: WindowAppearance) -> bool {
    match preference {
        ThemePreference::System => matches!(
            system_appearance,
            WindowAppearance::Dark | WindowAppearance::VibrantDark
        ),
        ThemePreference::Light => false,
        ThemePreference::Dark => true,
    }
}

fn native_override(preference: ThemePreference) -> Option<bool> {
    match preference {
        ThemePreference::System => None,
        ThemePreference::Light => Some(false),
        ThemePreference::Dark => Some(true),
    }
}

/// Insulator's visual language, take two: neutral graphite surfaces in the spirit
/// of Cursor — color is reserved for meaning. On macOS the sidebar's semantic
/// tint is installed as a native layer above Sidebar vibrancy; keeping this
/// GPUI surface clear avoids incorrectly accumulating the alpha of nested Metal
/// backgrounds. Selected, hovered, and pressed rows remain a 6% neutral layer.
#[derive(Clone, Copy)]
pub struct Theme {
    pub is_dark: bool,
    pub canvas: Hsla,
    pub sidebar: Hsla,
    pub sidebar_drag_background: Hsla,
    pub sidebar_item_background: Hsla,
    pub surface: Hsla,
    pub raised: Hsla,
    /// Solid elevated surface for floating cards, popovers, and menus.
    pub elevated: Hsla,
    /// Solid surface for inner containers inside popovers and floating panels.
    pub elevated_surface: Hsla,
    pub composer: Hsla,
    pub inset: Hsla,
    /// Terminal screen surface: paper-white in light mode, near-black in dark.
    pub terminal: Hsla,
    pub overlay: Hsla,
    pub overlay_strong: Hsla,

    pub border: Hsla,
    pub border_strong: Hsla,
    pub sidebar_border: Hsla,

    pub text: Hsla,
    pub text_secondary: Hsla,
    pub text_tertiary: Hsla,
    pub text_ghost: Hsla,

    /// Brand coral. Logo, caret, live-activity pulses — nothing structural.
    pub accent: Hsla,
    pub primary: Hsla,
    pub resize_handle: Hsla,
    /// Meter fills in the usage panel. Quota-meter blue by convention;
    /// warning/danger take over as a lane fills.
    pub gauge: Hsla,

    /// Text-selection wash. Painted *under* the glyphs, so it stays
    /// translucent and deliberately reads as the familiar browser blue rather
    /// than as brand color.
    pub selection: Hsla,
    /// Inline `code` foreground and its rounded wash.
    pub code_text: Hsla,
    pub code_wash: Hsla,

    /// Light fill for primary buttons (send, allow), dark glyph on top.
    pub inverse: Hsla,
    pub on_inverse: Hsla,

    pub warning: Hsla,
    pub success: Hsla,
    pub favorite: Hsla,
    pub danger: Hsla,
    pub danger_soft: Hsla,
}

impl Theme {
    pub fn current(cx: &App) -> Self {
        if cx.has_global::<ActiveInsulatorTheme>() {
            cx.global::<ActiveInsulatorTheme>().0
        } else {
            Self::dark()
        }
    }
}

pub trait ActiveTheme {
    fn theme(&self) -> Theme;
}

impl ActiveTheme for App {
    fn theme(&self) -> Theme {
        Theme::current(self)
    }
}

impl<T> ActiveTheme for Context<'_, T> {
    fn theme(&self) -> Theme {
        Theme::current(self)
    }
}

impl Theme {
    pub fn from_color_theme(color_theme: ColorTheme) -> Self {
        // Preserve the original Insulator graphite palettes as selectable themes,
        // including their native sidebar, overlay, and semantic colors.
        match color_theme {
            ColorTheme::InsulatorLight => return Self::light(),
            ColorTheme::InsulatorDark => return Self::dark(),
            _ => {}
        }
        let palette = match color_theme {
            ColorTheme::CatppuccinLatte => (
                0xfaf4ed, 0xfffaf3, 0xf2e9e1, 0x4c4f69, 0x6c6f85, 0x8839ef, 0x40a02b, 0xdf8e1d,
                0xd20f39, false,
            ),
            ColorTheme::CatppuccinFrappe => (
                0x303446, 0x292c3c, 0x414559, 0xc6d0f5, 0x949cbb, 0xca9ee6, 0xa6d189, 0xe5c890,
                0xe78284, true,
            ),
            ColorTheme::CatppuccinMacchiato => (
                0x24273a, 0x1e2030, 0x363a4f, 0xcad3f5, 0x939ab7, 0xc6a0f6, 0xa6da95, 0xeed49f,
                0xed8796, true,
            ),
            ColorTheme::CatppuccinMocha => (
                0x1e1e2e, 0x181825, 0x313244, 0xcdd6f4, 0x9399b2, 0xcba6f7, 0xa6e3a1, 0xf9e2af,
                0xf38ba8, true,
            ),
            ColorTheme::TokyoNight => (
                0x1a1b26, 0x16161e, 0x292e42, 0xc0caf5, 0xa9b1d6, 0x7aa2f7, 0x9ece6a, 0xe0af68,
                0xf7768e, true,
            ),
            ColorTheme::TokyoStorm => (
                0x24283b, 0x1f2335, 0x2f3549, 0xc0caf5, 0xa9b1d6, 0x7aa2f7, 0x9ece6a, 0xe0af68,
                0xf7768e, true,
            ),
            ColorTheme::TokyoMoon => (
                0x222436, 0x1e2030, 0x2d3045, 0xc8d3f5, 0xa9b1d6, 0x82aaff, 0xc3e88d, 0xffc777,
                0xff757f, true,
            ),
            ColorTheme::TokyoDay => (
                0xe1e2e7, 0xd5d6db, 0xc4c5ca, 0x3760bf, 0x52648a, 0x2e7de9, 0x587539, 0x8c6c3e,
                0xf52a65, false,
            ),
            ColorTheme::RosePine => (
                0x191724, 0x1f1d2e, 0x26233a, 0xe0def4, 0x908caa, 0x9ccfd8, 0x31748f, 0xf6c177,
                0xeb6f92, true,
            ),
            ColorTheme::RosePineMoon => (
                0x232136, 0x2a273f, 0x393552, 0xe0def4, 0x908caa, 0x9ccfd8, 0x3e8fb0, 0xf6c177,
                0xeb6f92, true,
            ),
            ColorTheme::RosePineDawn => (
                0xfaf4ed, 0xfffaf3, 0xf2e9e1, 0x575279, 0x9893a5, 0x56949f, 0x286983, 0xea9d34,
                0xb4637a, false,
            ),
            ColorTheme::GruvboxDark => (
                0x282828, 0x3c3836, 0x504945, 0xebdbb2, 0xa89984, 0x83a598, 0xb8bb26, 0xfabd2f,
                0xfb4934, true,
            ),
            ColorTheme::GruvboxLight => (
                0xfbf1c7, 0xf2e5bc, 0xebdbb2, 0x3c3836, 0x7c6f64, 0x076678, 0x79740e, 0xb57614,
                0x9d0006, false,
            ),
            ColorTheme::Vesper => (
                0x101010, 0x181818, 0x252525, 0xffffff, 0xa0a0a0, 0x99ffe4, 0x99ffe4, 0xffc799,
                0xff8080, true,
            ),
            ColorTheme::KanagawaWave => (
                0x1f1f28, 0x16161d, 0x2a2a37, 0xdcd7ba, 0x727169, 0x7e9cd8, 0x98bb6c, 0xe6c384,
                0xe46876, true,
            ),
            ColorTheme::KanagawaLotus => (
                0xf2ecbc, 0xe7dba0, 0xddd3a5, 0x545464, 0x8a8980, 0x4d699b, 0x6f894e, 0xcc6d00,
                0xc84053, false,
            ),
            ColorTheme::Nord => (
                0x2e3440, 0x3b4252, 0x434c5e, 0xe5e9f0, 0x81a1c1, 0x88c0d0, 0xa3be8c, 0xebcb8b,
                0xbf616a, true,
            ),
            ColorTheme::Dracula => (
                0x282a36, 0x21222c, 0x44475a, 0xf8f8f2, 0xa6a9c4, 0xbd93f9, 0x50fa7b, 0xf1fa8c,
                0xff5555, true,
            ),
            ColorTheme::OneDark => (
                0x282c34, 0x21252b, 0x3e4451, 0xabb2bf, 0x9da5b4, 0x61afef, 0x98c379, 0xe5c07b,
                0xe06c75, true,
            ),
            ColorTheme::SolarizedDark => (
                0x002b36, 0x073642, 0x0b4652, 0x839496, 0x93a1a1, 0x268bd2, 0x859900, 0xb58900,
                0xdc322f, true,
            ),
            ColorTheme::SolarizedLight => (
                0xfdf6e3, 0xeee8d5, 0xe5dfc9, 0x657b83, 0x586e75, 0x268bd2, 0x859900, 0xb58900,
                0xdc322f, false,
            ),
            ColorTheme::EverforestDark => (
                0x2d353b, 0x343f44, 0x3d484d, 0xd3c6aa, 0x859289, 0xa7c080, 0xa7c080, 0xdbbc7f,
                0xe67e80, true,
            ),
            ColorTheme::EverforestLight => (
                0xfdf6e3, 0xf4f0d9, 0xe8e4cf, 0x5c6a72, 0x939f91, 0x8da101, 0x8da101, 0xdfa000,
                0xf85552, false,
            ),
            ColorTheme::InsulatorLight | ColorTheme::InsulatorDark => {
                unreachable!("Insulator base themes return above")
            }
        };
        let (canvas, surface, raised, text, muted, accent, success, warning, danger, is_dark) =
            palette;
        let sidebar_surface = surface;
        let sidebar = if cfg!(target_os = "macos") {
            transparent_black()
        } else {
            rgb(sidebar_surface).into()
        };
        let base_overlay = if is_dark {
            hsla(0.0, 0.0, 1.0, 0.06)
        } else {
            hsla(0.0, 0.0, 0.0, 0.06)
        };
        let elevated_color = if is_dark && raised == 0x44475a {
            rgb(0x282a36).into()
        } else {
            rgb(raised).into()
        };
        Self {
            is_dark,
            canvas: rgb(canvas).into(),
            sidebar,
            sidebar_drag_background: rgb(sidebar_surface).into(),
            sidebar_item_background: base_overlay,
            surface: rgb(surface).into(),
            raised: rgb(raised).into(),
            elevated: elevated_color,
            elevated_surface: rgb(sidebar_surface).into(),
            composer: rgb(raised).into(),
            inset: rgb(if is_dark { canvas } else { raised }).into(),
            terminal: rgb(canvas).into(),
            overlay: base_overlay,
            overlay_strong: base_overlay.opacity(1.5),
            border: base_overlay,
            border_strong: base_overlay.opacity(2.0),
            sidebar_border: base_overlay,
            text: rgb(text).into(),
            text_secondary: rgb(muted).into(),
            text_tertiary: rgb(muted).into(),
            text_ghost: rgb(muted).into(),
            accent: rgb(accent).into(),
            primary: rgb(accent).into(),
            resize_handle: rgb(accent).into(),
            gauge: rgb(accent).into(),
            selection: rgb(accent).into(),
            code_text: rgb(warning).into(),
            code_wash: base_overlay,
            inverse: rgb(text).into(),
            on_inverse: rgb(canvas).into(),
            warning: rgb(warning).into(),
            success: rgb(success).into(),
            favorite: rgb(warning).into(),
            danger: rgb(danger).into(),
            danger_soft: rgb(danger).into(),
        }
    }

    pub fn dark() -> Self {
        Self {
            is_dark: true,
            canvas: rgb(0x111112).into(),
            sidebar: if cfg!(target_os = "macos") {
                transparent_black()
            } else {
                rgb(0x0C0C0D).into()
            },
            sidebar_drag_background: rgb(0x0C0C0D).into(),
            sidebar_item_background: hsla(0.0, 0.0, 1.0, 0.045),
            surface: rgb(0x111112).into(),
            raised: rgb(0x19191A).into(),
            elevated: rgb(0x1A1A1B).into(),
            elevated_surface: rgb(0x141415).into(),
            composer: rgb(0x151516).into(),
            inset: rgb(0x0D0D0E).into(),
            terminal: rgb(0x0D0D0E).into(),
            overlay: hsla(0.0, 0.0, 1.0, 0.04),
            overlay_strong: hsla(0.0, 0.0, 1.0, 0.075),

            border: hsla(0.0, 0.0, 1.0, 0.07),
            border_strong: hsla(0.0, 0.0, 1.0, 0.13),
            sidebar_border: hsla(0.0, 0.0, 1.0, 0.07),

            text: rgb(0xEDEDED).into(),
            text_secondary: rgb(0x9C9C9C).into(),
            text_tertiary: rgb(0x6F6F6F).into(),
            text_ghost: rgb(0x4A4A4A).into(),

            accent: rgb(0xE2795B).into(),
            primary: rgb(0xE2795B).into(),
            resize_handle: rgb(0x3B82F6).into(),
            gauge: rgb(0x3B82F6).into(),

            selection: hsla(211.0 / 360.0, 1.0, 0.50, 0.55),
            code_text: rgb(0xE0A882).into(),
            code_wash: hsla(0.0, 0.0, 1.0, 0.06),

            inverse: rgb(0xE7E9EC).into(),
            on_inverse: rgb(0x17181C).into(),

            warning: rgb(0xE0B36A).into(),
            success: rgb(0x62C987).into(),
            favorite: rgb(0xEAB308).into(),
            danger: rgb(0xE2726A).into(),
            danger_soft: hsla(4.0 / 360.0, 0.55, 0.63, 0.10),
        }
    }

    pub fn light() -> Self {
        Self {
            is_dark: false,
            canvas: rgb(0xFCFCFB).into(),
            sidebar: if cfg!(target_os = "macos") {
                transparent_black()
            } else {
                rgb(0xF2F2F1).into()
            },
            sidebar_drag_background: rgb(0xF2F2F1).into(),
            sidebar_item_background: hsla(0.0, 0.0, 0.0, 0.05),
            surface: rgb(0xFCFCFB).into(),
            raised: rgb(0xF1F1F0).into(),
            elevated: rgb(0xFFFFFF).into(),
            elevated_surface: rgb(0xF7F7F6).into(),
            composer: rgb(0xFFFFFF).into(),
            inset: rgb(0xEDEDEC).into(),
            terminal: rgb(0xFFFFFF).into(),
            overlay: hsla(0.0, 0.0, 0.0, 0.04),
            overlay_strong: hsla(0.0, 0.0, 0.0, 0.075),

            border: hsla(0.0, 0.0, 0.0, 0.08),
            border_strong: hsla(0.0, 0.0, 0.0, 0.14),
            sidebar_border: hsla(0.0, 0.0, 0.0, 0.08),

            text: rgb(0x161616).into(),
            text_secondary: rgb(0x5F5F5F).into(),
            text_tertiary: rgb(0x858585).into(),
            text_ghost: rgb(0xA4A4A4).into(),

            accent: rgb(0xC85F44).into(),
            primary: rgb(0xC85F44).into(),
            resize_handle: rgb(0x2563EB).into(),
            gauge: rgb(0x2563EB).into(),

            selection: hsla(211.0 / 360.0, 1.0, 0.50, 0.35),
            code_text: rgb(0x9A5528).into(),
            code_wash: hsla(0.0, 0.0, 0.0, 0.05),

            inverse: rgb(0x202227).into(),
            on_inverse: rgb(0xF8F8F9).into(),

            warning: rgb(0xA66B20).into(),
            success: rgb(0x2F8F52).into(),
            favorite: rgb(0xCA8A04).into(),
            danger: rgb(0xC64A42).into(),
            danger_soft: hsla(4.0 / 360.0, 0.55, 0.52, 0.10),
        }
    }
}

impl Theme {
    /// Resolve the sidebar surface while keeping the content controls fully
    /// opaque. Transparency is user-facing percentage: 0% is opaque and 100%
    /// lets the window background show through.
    pub fn sidebar_background(self, style: WindowStyle, transparency: f32, dragging: bool) -> Hsla {
        let background = if dragging {
            self.sidebar_drag_background
        } else {
            match style {
                WindowStyle::LiquidGlass => Hsla {
                    a: 0.08,
                    ..self.surface
                },
                WindowStyle::Image => Hsla {
                    a: 0.82,
                    ..self.canvas
                },
                WindowStyle::Solid => self.sidebar,
                WindowStyle::Transparent => self.surface,
            }
        };
        // Keep the sidebar surface in the render tree. Transparent mode uses
        // the window's raw background, not the liquid-glass blur layer.
        let visibility = if style == WindowStyle::Transparent {
            1.0
        } else if transparency.is_finite() {
            1.0 - transparency.clamp(0.0, 100.0) / 100.0
        } else {
            1.0 - insulator_client::persistence::DEFAULT_SIDEBAR_TRANSPARENCY / 100.0
        };
        Hsla {
            a: background.a * visibility,
            ..background
        }
    }

    pub fn for_window_style(mut self, style: WindowStyle, transparency: f32) -> Self {
        match style {
            WindowStyle::LiquidGlass => {
                let glass = |alpha: f32| (alpha * glass_opacity_factor()).min(1.0);
                let alpha_raised = glass(if self.is_dark { 0.22 } else { 0.32 });
                let alpha_composer = glass(if self.is_dark { 0.26 } else { 0.38 });
                let alpha_inset = glass(if self.is_dark { 0.20 } else { 0.28 });
                let alpha_surface = glass(if self.is_dark { 0.16 } else { 0.25 });
                let alpha_canvas = glass(if self.is_dark { 0.10 } else { 0.18 });
                self.raised = Hsla {
                    a: alpha_raised,
                    ..self.raised
                };
                self.composer = Hsla {
                    a: alpha_composer,
                    ..self.composer
                };
                self.inset = Hsla {
                    a: alpha_inset,
                    ..self.inset
                };
                self.surface = Hsla {
                    a: alpha_surface,
                    ..self.surface
                };
                self.canvas = Hsla {
                    a: alpha_canvas,
                    ..self.canvas
                };
                if self.is_dark {
                    self.elevated = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.22),
                        l: (self.canvas.l * 0.75 + 0.05).clamp(0.10, 0.16),
                        a: 0.78,
                    };
                    self.elevated_surface = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.22),
                        l: (self.canvas.l * 0.5 + 0.02).clamp(0.06, 0.11),
                        a: 0.82,
                    };
                    self.border_strong = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.15),
                        l: 0.90,
                        a: 0.14,
                    };
                } else {
                    self.elevated = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.15),
                        l: 0.96,
                        a: 0.80,
                    };
                    self.elevated_surface = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.15),
                        l: 0.92,
                        a: 0.84,
                    };
                    self.border_strong = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.12),
                        l: 0.10,
                        a: 0.12,
                    };
                }
                self.terminal = transparent_black();
            }
            WindowStyle::Image => {
                let alpha_raised = if self.is_dark { 0.25 } else { 0.35 };
                let alpha_composer = if self.is_dark { 0.25 } else { 0.35 };
                let alpha_inset = if self.is_dark { 0.20 } else { 0.30 };
                let alpha_surface = if self.is_dark { 0.18 } else { 0.28 };
                let alpha_canvas = if self.is_dark { 0.12 } else { 0.20 };
                self.raised = Hsla {
                    a: alpha_raised,
                    ..self.raised
                };
                self.composer = Hsla {
                    a: alpha_composer,
                    ..self.composer
                };
                self.inset = Hsla {
                    a: alpha_inset,
                    ..self.inset
                };
                self.surface = Hsla {
                    a: alpha_surface,
                    ..self.surface
                };
                self.canvas = Hsla {
                    a: alpha_canvas,
                    ..self.canvas
                };
                if self.is_dark {
                    self.elevated = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.22),
                        l: (self.canvas.l * 0.75 + 0.05).clamp(0.10, 0.16),
                        a: 0.80,
                    };
                    self.elevated_surface = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.22),
                        l: (self.canvas.l * 0.5 + 0.02).clamp(0.06, 0.11),
                        a: 0.84,
                    };
                    self.border_strong = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.15),
                        l: 0.90,
                        a: 0.16,
                    };
                } else {
                    self.elevated = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.15),
                        l: 0.96,
                        a: 0.82,
                    };
                    self.elevated_surface = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.15),
                        l: 0.92,
                        a: 0.86,
                    };
                    self.border_strong = Hsla {
                        h: self.canvas.h,
                        s: self.canvas.s.min(0.12),
                        l: 0.10,
                        a: 0.14,
                    };
                }
                self.terminal = transparent_black();
            }
            WindowStyle::Transparent => {
                // Transparent mode uses native vibrancy. Reduce the app's
                // tint as the setting increases while keeping blur active.
                let transparency = transparency.clamp(0.0, 100.0) / 100.0;
                let surface_visibility = 1.0 - 0.75 * transparency;
                // At 0% every app surface is opaque. As transparency rises,
                // the surfaces reveal the still-blurred desktop behind them.
                let alpha_raised =
                    1.0 - (1.0 - if self.is_dark { 0.32 } else { 0.52 }) * transparency;
                let alpha_composer =
                    1.0 - (1.0 - if self.is_dark { 0.36 } else { 0.58 }) * transparency;
                let alpha_inset =
                    1.0 - (1.0 - if self.is_dark { 0.26 } else { 0.44 }) * transparency;
                let alpha_surface =
                    1.0 - (1.0 - if self.is_dark { 0.22 } else { 0.34 }) * transparency;
                self.raised = Hsla {
                    a: alpha_raised * surface_visibility,
                    ..self.raised
                };
                self.composer = Hsla {
                    a: alpha_composer * surface_visibility,
                    ..self.composer
                };
                self.inset = Hsla {
                    a: alpha_inset * surface_visibility,
                    ..self.inset
                };
                self.surface = Hsla {
                    a: alpha_surface * surface_visibility,
                    ..self.surface
                };
                self.canvas = Hsla {
                    a: 1.0 - 0.92 * transparency,
                    ..self.canvas
                };
                self.elevated = Hsla {
                    a: 1.0 - (1.0 - if self.is_dark { 0.82 } else { 0.88 }) * transparency,
                    ..self.elevated
                };
                self.elevated_surface = Hsla {
                    a: 1.0 - (1.0 - if self.is_dark { 0.78 } else { 0.84 }) * transparency,
                    ..self.elevated_surface
                };
                self.terminal = Hsla {
                    h: self.terminal.h,
                    s: self.terminal.s,
                    l: self.terminal.l,
                    a: if self.is_dark { 0.82 } else { 0.88 },
                };
                self.sidebar_border = transparent_black();
            }
            WindowStyle::Solid => {}
        }
        self
    }
}

impl Theme {
    /// Swap the palette's accent for the user's choice. Selection follows it
    /// as a wash so highlighted text reads in the same hue.
    pub fn with_accent(mut self, accent: AccentColor) -> Self {
        let Some(color) = accent.rgb(self.is_dark) else {
            return self;
        };
        let color: Hsla = rgb(color).into();
        self.accent = color;
        self.primary = color;
        self.resize_handle = color;
        self.gauge = color;
        self.selection = Hsla {
            a: if self.is_dark { 0.45 } else { 0.30 },
            ..color
        };
        self
    }
}

#[derive(Clone, Copy)]
struct ActiveInsulatorTheme(Theme);

impl Global for ActiveInsulatorTheme {}

/// Publish the resolved palette. [`Theme::current`] reads it back from the
/// global, which is how every view gets its colors.
fn set_active_theme(theme: Theme, cx: &mut App) {
    cx.set_global(ActiveInsulatorTheme(theme));
}

/// Resolve and publish the startup palette, before any window exists.
pub fn init(cx: &mut App) {
    let system_appearance = cx.window_appearance();
    let dark = resolves_to_dark(ThemePreference::System, system_appearance);
    set_active_theme(
        Theme::from_color_theme(ColorTheme::default_for_dark(dark)),
        cx,
    );
}

pub fn apply_theme_preference(
    preference: ThemePreference,
    color_theme: ColorTheme,
    accent: AccentColor,
    window_style: WindowStyle,
    sidebar_transparency: f32,
    window: &mut Window,
    cx: &mut App,
) {
    let override_pref = if window_style == WindowStyle::LiquidGlass {
        ThemePreference::Dark
    } else {
        preference
    };
    crate::platform::set_window_appearance(window, native_override(override_pref));
    let is_dark = if window_style == WindowStyle::LiquidGlass {
        true
    } else {
        resolves_to_dark(preference, cx.window_appearance())
    };
    let color_theme = if color_theme.for_dark(is_dark) {
        color_theme
    } else {
        ColorTheme::default_for_dark(is_dark)
    };
    let theme = Theme::from_color_theme(color_theme)
        .with_accent(accent)
        .for_window_style(window_style, sidebar_transparency);
    let sidebar_color = theme.sidebar_drag_background;
    set_active_theme(theme, cx);
    crate::platform::configure_sidebar_material(
        window,
        is_dark,
        sidebar_color,
        sidebar_transparency,
    );
    window.refresh();
}

pub fn update_active_theme(
    preference: ThemePreference,
    color_theme: ColorTheme,
    accent: AccentColor,
    window_style: WindowStyle,
    sidebar_transparency: f32,
    cx: &mut App,
) {
    let is_dark = if window_style == WindowStyle::LiquidGlass {
        true
    } else {
        resolves_to_dark(preference, cx.window_appearance())
    };
    let color_theme = if color_theme.for_dark(is_dark) {
        color_theme
    } else {
        ColorTheme::default_for_dark(is_dark)
    };
    let theme = Theme::from_color_theme(color_theme)
        .with_accent(accent)
        .for_window_style(window_style, sidebar_transparency);
    set_active_theme(theme, cx);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounding_scales_authored_radii_and_clamps() {
        set_corner_rounding(0.0);
        assert_eq!(radius(8.0), px(0.0));
        set_corner_rounding(50.0);
        assert_eq!(radius(8.0), px(8.0));
        set_corner_rounding(100.0);
        assert_eq!(radius(8.0), px(16.0));
        set_corner_rounding(250.0);
        assert_eq!(radius(8.0), px(16.0));
        set_corner_rounding(0.0);
    }

    #[test]
    fn glass_level_scales_tint_from_frosted_to_clear() {
        set_glass_level(50.0);
        assert!((liquid_glass_tint_alpha(true) - 0.45).abs() < 1e-6);
        set_glass_level(100.0);
        assert_eq!(liquid_glass_tint_alpha(true), 0.0);
        set_glass_level(0.0);
        assert!((liquid_glass_tint_alpha(true) - 0.9).abs() < 1e-6);
        set_glass_level(f32::NAN);
        assert!((liquid_glass_tint_alpha(false) - 0.35).abs() < 1e-6);
        set_glass_level(50.0);
    }
}
