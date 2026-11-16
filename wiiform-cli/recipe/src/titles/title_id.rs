// SPDX-License-Identifier: GPL-2.0-only
use std::{num::NonZeroU64, str::FromStr};

use anyhow::{Context, Result};
use hex::FromHex;
use schemars::JsonSchema;
use serde::Deserialize;

/// Nonzero Wii title ID written as 16 hex digits.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Deserialize, JsonSchema)]
#[serde(try_from = "String")]
#[schemars(!try_from, transparent)]
pub struct TitleId(
    #[schemars(
        with = "String",
        length(equal = 16),
        regex(pattern = "^[0-9a-fA-F]{16}$"),
        extend("not" = {"const": "0000000000000000"})
    )]
    NonZeroU64,
);

impl FromStr for TitleId {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let bytes =
            <[u8; 8]>::from_hex(value).context("expected a 16-digit hexadecimal title ID")?;
        NonZeroU64::new(u64::from_be_bytes(bytes))
            .map(Self)
            .context("title ID must be nonzero")
    }
}

impl TryFrom<String> for TitleId {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Self> {
        value.parse()
    }
}

impl std::fmt::Display for TitleId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

impl TitleId {
    pub(crate) const fn known(value: u64) -> Self {
        Self(NonZeroU64::new(value).expect("built-in title IDs are nonzero"))
    }

    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(id) => Some(Self(id)),
            None => None,
        }
    }

    /// The upper half, which names the kind of title (system, channel, ...).
    pub const fn kind(self) -> u32 {
        (self.0.get() >> 32) as u32
    }

    pub const fn as_u64(self) -> u64 {
        self.0.get()
    }
}
