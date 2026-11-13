// SPDX-License-Identifier: GPL-2.0-only
//! System Menu, first-run setup, and display settings.

mod country;
mod nickname;

pub use country::CountryCode;
pub use nickname::Nickname;

use std::str::FromStr;

use schemars::JsonSchema;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

/// Stock System Menu version used by the generated image.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
pub enum Menu {
    /// European 4.3 menu.
    #[serde(rename = "4.3E")]
    E43,
    /// North American 4.3 menu.
    #[serde(rename = "4.3U")]
    U43,
    /// Japanese 4.3 menu.
    #[serde(rename = "4.3J")]
    J43,
}

impl Menu {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::E43 => "4.3E",
            Self::U43 => "4.3U",
            Self::J43 => "4.3J",
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            Self::E43 => "50",
            Self::U43 => "45",
            Self::J43 => "4a",
        }
    }

    pub fn allows_language(self, language: Language) -> bool {
        match self {
            Self::E43 => matches!(
                language,
                Language::En
                    | Language::De
                    | Language::Fr
                    | Language::Es
                    | Language::It
                    | Language::Nl
            ),
            Self::U43 => matches!(language, Language::En | Language::Fr | Language::Es),
            Self::J43 => language == Language::Ja,
        }
    }

    pub fn allows_country(self, country: &CountryCode) -> bool {
        match self {
            Self::E43 => country.is_pal(),
            Self::U43 => country.is_americas(),
            Self::J43 => country.as_u8() == 1,
        }
    }

    pub fn default_country(self) -> CountryCode {
        match self {
            Self::E43 => CountryCode::united_kingdom(),
            Self::U43 => CountryCode::united_states(),
            Self::J43 => CountryCode::japan(),
        }
    }

    pub fn default_language(self) -> Language {
        match self {
            Self::E43 | Self::U43 => Language::En,
            Self::J43 => Language::Ja,
        }
    }
}

/// Wii language selected in the first-run settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Language {
    #[serde(rename = "ja")]
    Ja,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "de")]
    De,
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "es")]
    Es,
    #[serde(rename = "it")]
    It,
    #[serde(rename = "nl")]
    Nl,
}

/// First-run setup fields written to SYSCONF.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    /// Optional console nickname (1–10 printable BMP characters).
    #[schemars(with = "crate::schema::NonNull<Nickname>")]
    pub nickname: Option<Nickname>,
    /// Optional setup language, constrained by the effective menu region.
    #[schemars(with = "crate::schema::NonNull<Language>")]
    pub language: Option<Language>,
    /// Wii country code used for country-dependent console settings.
    #[schemars(with = "crate::schema::NonNull<CountryCode>")]
    pub country: Option<CountryCode>,
    /// Physical sensor bar position.
    #[schemars(with = "crate::schema::NonNull<SensorBar>")]
    pub sensor_bar: Option<SensorBar>,
}

impl Setup {
    pub fn country_for(&self, menu: Menu) -> CountryCode {
        self.country
            .clone()
            .unwrap_or_else(|| menu.default_country())
    }

    pub fn language_for(&self, menu: Menu) -> Language {
        self.language.unwrap_or_else(|| menu.default_language())
    }

    pub fn sensor_bar_for(&self) -> SensorBar {
        self.sensor_bar.unwrap_or(SensorBar::Above)
    }
}

/// Sensor bar position relative to the display.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum SensorBar {
    #[default]
    #[serde(rename = "above")]
    Above,
    #[serde(rename = "below")]
    Below,
}

/// Screen aspect ratio.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Aspect {
    #[serde(rename = "4:3")]
    FourThree,
    #[serde(rename = "16:9")]
    SixteenNine,
}

/// Display configuration written to SYSCONF.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Display {
    /// Set 4:3 or widescreen output.
    #[schemars(with = "crate::schema::NonNull<Aspect>")]
    pub aspect: Option<Aspect>,
    /// Enable progressive 480p output when supported by the display chain.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub progressive: Option<bool>,
    /// Enable PAL 60 Hz mode.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub pal60: Option<bool>,
}

impl Display {
    pub(crate) fn is_empty(&self) -> bool {
        self.aspect.is_none() && self.progressive.is_none() && self.pal60.is_none()
    }
}

impl Language {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ja => "ja",
            Self::En => "en",
            Self::De => "de",
            Self::Fr => "fr",
            Self::Es => "es",
            Self::It => "it",
            Self::Nl => "nl",
        }
    }
}

impl Aspect {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FourThree => "4:3",
            Self::SixteenNine => "16:9",
        }
    }
}

impl FromStr for Menu {
    type Err = serde::de::value::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::deserialize(value.into_deserializer())
    }
}

impl FromStr for Language {
    type Err = serde::de::value::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::deserialize(value.into_deserializer())
    }
}

impl FromStr for Aspect {
    type Err = serde::de::value::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::deserialize(value.into_deserializer())
    }
}
