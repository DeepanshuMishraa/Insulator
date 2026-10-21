//! Process-neutral theme preference persisted in the desktop settings file.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum WindowStyle {
    #[default]
    Solid,
    LiquidGlass,
    Image,
    Transparent,
}

impl WindowStyle {
    pub const ALL: [Self; 4] = [
        Self::Solid,
        Self::LiquidGlass,
        Self::Image,
        Self::Transparent,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::LiquidGlass => "Liquid Glass",
            Self::Image => "Image",
            Self::Transparent => "Transparent",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize, TS)]
#[ts(export)]
#[serde(rename_all = "snake_case")]
pub enum ColorTheme {
    InsulatorLight,
    #[default]
    InsulatorDark,
    CatppuccinLatte,
    CatppuccinFrappe,
    CatppuccinMacchiato,
    CatppuccinMocha,
    TokyoNight,
    TokyoStorm,
    TokyoMoon,
    TokyoDay,
    RosePine,
    RosePineMoon,
    RosePineDawn,
    GruvboxDark,
    GruvboxLight,
    Vesper,
    KanagawaWave,
    KanagawaLotus,
    Nord,
    Dracula,
    OneDark,
    SolarizedDark,
    SolarizedLight,
    EverforestDark,
    EverforestLight,
}

impl ColorTheme {
    pub const ALL: [Self; 25] = [
        Self::InsulatorLight,
        Self::InsulatorDark,
        Self::CatppuccinLatte,
        Self::CatppuccinFrappe,
        Self::CatppuccinMacchiato,
        Self::CatppuccinMocha,
        Self::TokyoNight,
        Self::TokyoStorm,
        Self::TokyoMoon,
        Self::TokyoDay,
        Self::RosePine,
        Self::RosePineMoon,
        Self::RosePineDawn,
        Self::GruvboxDark,
        Self::GruvboxLight,
        Self::Vesper,
        Self::KanagawaWave,
        Self::KanagawaLotus,
        Self::Nord,
        Self::Dracula,
        Self::OneDark,
        Self::SolarizedDark,
        Self::SolarizedLight,
        Self::EverforestDark,
        Self::EverforestLight,
    ];

    pub fn is_dark(self) -> bool {
        !matches!(
            self,
            Self::InsulatorLight
                | Self::CatppuccinLatte
                | Self::TokyoDay
                | Self::RosePineDawn
                | Self::GruvboxLight
                | Self::KanagawaLotus
                | Self::SolarizedLight
                | Self::EverforestLight
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::InsulatorLight => "Insulator Light",
            Self::InsulatorDark => "Insulator Dark",
            Self::CatppuccinLatte => "Catppuccin Latte",
            Self::CatppuccinFrappe => "Catppuccin Frappé",
            Self::CatppuccinMacchiato => "Catppuccin Macchiato",
            Self::CatppuccinMocha => "Catppuccin Mocha",
            Self::TokyoNight => "Tokyo Night",
            Self::TokyoStorm => "Tokyo Storm",
            Self::TokyoMoon => "Tokyo Moon",
            Self::TokyoDay => "Tokyo Day",
            Self::RosePine => "Rosé Pine",
            Self::RosePineMoon => "Rosé Pine Moon",
            Self::RosePineDawn => "Rosé Pine Dawn",
            Self::GruvboxDark => "Gruvbox Dark",
            Self::GruvboxLight => "Gruvbox Light",
            Self::Vesper => "Vesper",
            Self::KanagawaWave => "Kanagawa Wave",
            Self::KanagawaLotus => "Kanagawa Lotus",
            Self::Nord => "Nord",
            Self::Dracula => "Dracula",
            Self::OneDark => "One Dark",
            Self::SolarizedDark => "Solarized Dark",
            Self::SolarizedLight => "Solarized Light",
            Self::EverforestDark => "Everforest Dark",
            Self::EverforestLight => "Everforest Light",
        }
    }

    pub fn for_dark(self, dark: bool) -> bool {
        self.is_dark() == dark
    }

    pub fn default_for_dark(dark: bool) -> Self {
        if dark {
            Self::InsulatorDark
        } else {
            Self::InsulatorLight
        }
    }
}

/// Accent laid over the color theme. `Theme` keeps the palette's own accent.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AccentColor {
    #[default]
    Theme,
    Coral,
    Orange,
    Amber,
    Lime,
    Green,
    Teal,
    Cyan,
    Blue,
    Indigo,
    Violet,
    Pink,
    Red,
    Mono,
}

impl AccentColor {
    pub const ALL: [Self; 14] = [
        Self::Theme,
        Self::Coral,
        Self::Orange,
        Self::Amber,
        Self::Lime,
        Self::Green,
        Self::Teal,
        Self::Cyan,
        Self::Blue,
        Self::Indigo,
        Self::Violet,
        Self::Pink,
        Self::Red,
        Self::Mono,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Coral => "Coral",
            Self::Orange => "Orange",
            Self::Amber => "Amber",
            Self::Lime => "Lime",
            Self::Green => "Green",
            Self::Teal => "Teal",
            Self::Cyan => "Cyan",
            Self::Blue => "Blue",
            Self::Indigo => "Indigo",
            Self::Violet => "Violet",
            Self::Pink => "Pink",
            Self::Red => "Red",
            Self::Mono => "Mono",
        }
    }

    /// The accent's RGB for a dark or light surface, or `None` to keep the
    /// color theme's own.
    pub fn rgb(self, dark: bool) -> Option<u32> {
        let (on_dark, on_light) = match self {
            Self::Theme => return None,
            Self::Coral => (0xE2795B, 0xC85F44),
            Self::Orange => (0xF08A3C, 0xC2560F),
            Self::Amber => (0xE8B04A, 0xA16207),
            Self::Lime => (0xA3D45A, 0x4D7C0F),
            Self::Green => (0x5CC98A, 0x15803D),
            Self::Teal => (0x4FC4B4, 0x0F766E),
            Self::Cyan => (0x5CC8E8, 0x0E7490),
            Self::Blue => (0x5B9CF5, 0x1D4ED8),
            Self::Indigo => (0x8A90F5, 0x4338CA),
            Self::Violet => (0xA98BF5, 0x6D28D9),
            Self::Pink => (0xEC76B0, 0xBE185D),
            Self::Red => (0xEE6B6B, 0xB91C1C),
            Self::Mono => (0xE6E6E6, 0x1F1F1F),
        };
        Some(if dark { on_dark } else { on_light })
    }
}

impl ThemePreference {
    pub const ALL: [Self; 3] = [Self::System, Self::Light, Self::Dark];

    pub fn label(self) -> String {
        match self {
            Self::System => crate::i18n::translate("settings.theme_system"),
            Self::Light => crate::i18n::translate("settings.theme_light"),
            Self::Dark => crate::i18n::translate("settings.theme_dark"),
        }
    }
}
