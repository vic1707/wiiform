// SPDX-License-Identifier: GPL-2.0-only
//! First-run setup: the `setup` key.
mod country;
mod display;

use nnn::nnn;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use self::{
    country::CountryCode,
    display::{Aspect, Display},
};
use crate::Menu;

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
