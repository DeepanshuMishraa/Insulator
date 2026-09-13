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
