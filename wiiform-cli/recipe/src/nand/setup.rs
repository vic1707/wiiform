// SPDX-License-Identifier: GPL-2.0-only
//! First-run setup: the `setup` key.
pub mod display;

mod country;
mod nickname;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use self::{country::CountryCode, display::Display, nickname::Nickname};

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

/// First-run setup values written to SYSCONF. The console's locale is
/// required; the other values default to what a factory-fresh Wii holds.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    /// Console nickname. Defaults to the Wii's factory name.
    #[serde(default)]
    pub nickname: Nickname,
    /// Setup language: en, de, fr, es, it or nl on 4.3E; en, fr or es on 4.3U;
    /// ja on 4.3J.
    pub language: Language,
    /// Wii-supported ISO alpha-2 code of a country in the menu's region, such
    /// as GB, FR, US, or JP. Quote "NO" in YAML.
    pub country: CountryCode,
    /// Physical sensor bar position. Defaults to above the display.
    #[serde(default)]
    pub sensor_bar: SensorBar,
    /// Display settings. Omit this mapping to use the display defaults.
    #[serde(default)]
    pub display: Display,
}

/// Sensor bar position relative to the display.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SensorBar {
    #[default]
    Above,
    Below,
}
