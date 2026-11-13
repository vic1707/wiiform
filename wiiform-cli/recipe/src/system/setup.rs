// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::{CountryCode, Language, Menu, Nickname};

/// First-run setup values written to SYSCONF.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Setup {
    /// Console nickname. Defaults to the Wii's factory name.
    pub nickname: Nickname,
    /// Setup language, constrained by the effective menu region.
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<Language>")]
    pub language: Option<Language>,
    /// Country code used for country-dependent console settings.
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<CountryCode>")]
    pub country: Option<CountryCode>,
    /// Physical sensor bar position. Defaults to above the display.
    pub sensor_bar: SensorBar,
}

impl Setup {
    pub fn country_for(&self, menu: Menu) -> CountryCode {
        self.country.unwrap_or_else(|| menu.default_country())
    }

    pub fn language_for(&self, menu: Menu) -> Language {
        self.language.unwrap_or_else(|| menu.default_language())
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
