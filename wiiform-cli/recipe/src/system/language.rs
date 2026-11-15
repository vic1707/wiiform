// SPDX-License-Identifier: GPL-2.0-only
use std::str::FromStr;

use schemars::JsonSchema;
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};

/// Wii first-run language value written to SYSCONF.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[repr(u8)]
pub enum Language {
    #[serde(rename = "ja")]
    Ja = 0,
    #[serde(rename = "en")]
    En = 1,
    #[serde(rename = "de")]
    De = 2,
    #[serde(rename = "fr")]
    Fr = 3,
    #[serde(rename = "es")]
    Es = 4,
    #[serde(rename = "it")]
    It = 5,
    #[serde(rename = "nl")]
    Nl = 6,
}

impl Language {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }
}

impl FromStr for Language {
    type Err = serde::de::value::Error;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::deserialize(value.into_deserializer())
    }
}
