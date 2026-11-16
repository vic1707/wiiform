// SPDX-License-Identifier: GPL-2.0-only
//! System Menu, first-run setup, and display settings.
use nnn::nnn;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::CountryCode;

/// Stock System Menu version used by the generated image.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Menu {
    /// European 4.3 menu.
    #[serde(rename = "4.3E")]
    #[cfg_attr(feature = "cli", value(name = "4.3E"))]
    E43,
    /// North American 4.3 menu.
    #[serde(rename = "4.3U")]
    #[cfg_attr(feature = "cli", value(name = "4.3U"))]
    U43,
    /// Japanese 4.3 menu.
    #[serde(rename = "4.3J")]
    #[cfg_attr(feature = "cli", value(name = "4.3J"))]
    J43,
}

/// Region code a title declares in its TMD.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    Japan,
    Usa,
    Pal,
    /// Runs on every console.
    Free,
    Korea,
}

impl Region {
    pub const fn from_code(code: u16) -> Option<Self> {
        Some(match code {
            0 => Self::Japan,
            1 => Self::Usa,
            2 => Self::Pal,
            3 => Self::Free,
            4 => Self::Korea,
            _ => return None,
        })
    }
}

impl std::fmt::Display for Menu {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::E43 => "4.3E",
            Self::U43 => "4.3U",
            Self::J43 => "4.3J",
        })
    }
}

impl Menu {
    pub const fn region(self) -> Region {
        match self {
            Self::E43 => Region::Pal,
            Self::U43 => Region::Usa,
            Self::J43 => Region::Japan,
        }
    }

    /// Last character of this region's channel title IDs.
    pub(crate) const fn channel_suffix(self) -> u8 {
        match self {
            Self::E43 => b'P',
            Self::U43 => b'E',
            Self::J43 => b'J',
        }
    }

    pub(crate) fn allows_language(self, language: Language) -> bool {
        use Language::*;
        match self {
            Self::E43 => matches!(language, En | De | Fr | Es | It | Nl),
            Self::U43 => matches!(language, En | Fr | Es),
            Self::J43 => language == Ja,
        }
    }

    pub(crate) fn allows_country(self, country: CountryCode) -> bool {
        match self {
            Self::E43 => country.is_pal(),
            Self::U43 => country.is_americas(),
            Self::J43 => country == CountryCode::Japan,
        }
    }
}

/// Wii first-run language.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    Ja,
    En,
    De,
    Fr,
    Es,
    It,
    Nl,
}

/// First-run setup values written to SYSCONF.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Setup {
    /// Console nickname. Defaults to the Wii's factory name.
    pub nickname: Nickname,
    /// Setup language, constrained by the effective menu region.
    /// Defaults to en for 4.3E and 4.3U, and ja for 4.3J.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<Language>,
    /// Wii-supported ISO alpha-2 code, such as GB, US, or JP. Quote "NO" in YAML.
    /// Defaults to GB, US, or JP for the menu's region.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub country: Option<CountryCode>,
    /// Physical sensor bar position. Defaults to above the display.
    pub sensor_bar: SensorBar,
    /// Display settings. Omit this mapping to use the display defaults.
    pub display: Display,
}

impl Setup {
    pub fn language_for(&self, menu: Menu) -> Language {
        self.language.unwrap_or(match menu {
            Menu::E43 | Menu::U43 => Language::En,
            Menu::J43 => Language::Ja,
        })
    }

    pub fn country_for(&self, menu: Menu) -> CountryCode {
        self.country.unwrap_or(match menu {
            Menu::E43 => CountryCode::UnitedKingdom,
            Menu::U43 => CountryCode::UnitedStates,
            Menu::J43 => CountryCode::Japan,
        })
    }
}

#[nnn(
    attrs(
        doc = "Console nickname with 1–10 BMP letters, numbers, or spaces, including a letter or number.",
        schemars(extend("minLength" = 1, "maxLength" = 10))
    ),
    derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema),
    nnn_derive(TryFrom, AsRef<str>),
    default = "Wii".to_owned(),
    validators(
        custom(with = |name: &str| !name.trim().is_empty(), error = Nonblank),
        custom(with = |name: &str| name.chars().nth(10).is_none(), error = AtMostTenCharacters),
        custom(
            with = |name: &str| name.chars().all(|ch| {
                ch.len_utf16() == 1 && (ch.is_alphanumeric() || ch == ' ')
            }),
            error = InvalidChar
        )
    )
)]
pub struct Nickname(String);

/// Sensor bar position relative to the display.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SensorBar {
    #[default]
    Above,
    Below,
}

/// Screen aspect ratio.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Aspect {
    #[default]
    #[serde(rename = "4:3")]
    FourThree,
    #[serde(rename = "16:9")]
    SixteenNine,
}

/// Display configuration applied during first-run setup.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Display {
    /// Set 4:3 or widescreen output (defaults to 4:3).
    pub aspect: Aspect,
    /// Enable progressive 480p output when supported (defaults to off).
    pub progressive: bool,
    /// Enable PAL 60 Hz mode (defaults to off).
    pub pal60: bool,
}
