// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use sha2::Digest;

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
    pub [u8; 32],
);

impl std::str::FromStr for Sha256 {
    type Err = hex::FromHexError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        hex::FromHex::from_hex(value).map(Self)
    }
}

impl Sha256 {
    pub fn of(bytes: &[u8]) -> Self {
        Self(sha2::Sha256::digest(bytes).into())
    }
}
