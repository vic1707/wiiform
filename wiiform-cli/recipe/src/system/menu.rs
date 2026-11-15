// SPDX-License-Identifier: GPL-2.0-only
use std::str::FromStr;

use schemars::JsonSchema;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

use super::{CountryCode, Language};

/// Stock System Menu version used by the generated image.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
#[repr(u16)]
pub enum Menu {
    /// European 4.3 menu.
    #[serde(rename = "4.3E")]
    E43 = 514,
    /// North American 4.3 menu.
    #[serde(rename = "4.3U")]
    U43 = 513,
    /// Japanese 4.3 menu.
    #[serde(rename = "4.3J")]
    J43 = 512,
}

impl Menu {
    pub const fn version(self) -> u16 {
        self as u16
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::E43 => "4.3E",
            Self::U43 => "4.3U",
            Self::J43 => "4.3J",
        }
    }

    pub const fn channel_suffix(self) -> u8 {
        match self {
            Self::E43 => b'P',
            Self::U43 => b'E',
            Self::J43 => b'J',
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

    pub fn allows_country(self, country: CountryCode) -> bool {
        match self {
            Self::E43 => country.is_pal(),
            Self::U43 => country.is_americas(),
            Self::J43 => country == CountryCode::Japan,
        }
    }

    pub const fn default_country(self) -> CountryCode {
        match self {
            Self::E43 => CountryCode::UnitedKingdom,
            Self::U43 => CountryCode::UnitedStates,
            Self::J43 => CountryCode::Japan,
        }
    }

    pub const fn default_language(self) -> Language {
        match self {
            Self::E43 | Self::U43 => Language::En,
            Self::J43 => Language::Ja,
        }
    }
}

impl FromStr for Menu {
    type Err = serde::de::value::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::deserialize(value.into_deserializer())
    }
}
