// SPDX-License-Identifier: GPL-2.0-only
use std::str::FromStr;

use schemars::JsonSchema;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

/// Screen aspect ratio.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Aspect {
    #[default]
    #[serde(rename = "4:3")]
    FourThree,
    #[serde(rename = "16:9")]
    SixteenNine,
}

impl FromStr for Aspect {
    type Err = serde::de::value::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::deserialize(value.into_deserializer())
    }
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
