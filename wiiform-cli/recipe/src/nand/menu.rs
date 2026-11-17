// SPDX-License-Identifier: GPL-2.0-only
//! The stock System Menu a recipe targets.
use derive_more::Display;
use schemars::JsonSchema;
use serde::Deserialize;

use super::setup::{CountryCode, Language};
use crate::Region;

/// Stock System Menu version used by the generated image.
#[derive(Clone, Copy, Debug, Display, Eq, PartialEq, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
pub enum Menu {
    /// European 4.3 menu.
    #[display("4.3E")]
    #[serde(rename = "4.3E")]
    #[cfg_attr(feature = "cli", value(name = "4.3E"))]
    E43,
    /// North American 4.3 menu.
    #[display("4.3U")]
    #[serde(rename = "4.3U")]
    #[cfg_attr(feature = "cli", value(name = "4.3U"))]
    U43,
    /// Japanese 4.3 menu.
    #[display("4.3J")]
    #[serde(rename = "4.3J")]
    #[cfg_attr(feature = "cli", value(name = "4.3J"))]
    J43,
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
