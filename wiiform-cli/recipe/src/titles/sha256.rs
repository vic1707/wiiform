// SPDX-License-Identifier: GPL-2.0-only
use std::str::FromStr;

use hex::FromHex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// SHA-256 of the exact binary payload, encoded as 64 hex digits.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct Sha256(
    #[serde(with = "hex")]
    #[schemars(
        with = "String",
        length(equal = 64),
        regex(pattern = "^[0-9a-fA-F]{64}$")
    )]
    [u8; 32],
);

impl FromStr for Sha256 {
    type Err = hex::FromHexError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        <[u8; 32]>::from_hex(value).map(Self)
    }
}

impl From<[u8; 32]> for Sha256 {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

impl Sha256 {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}
