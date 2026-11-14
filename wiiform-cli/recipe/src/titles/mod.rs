// SPDX-License-Identifier: GPL-2.0-only
//! WAD requests, title identifiers, and channel removals.

mod removal;
mod sha256;
mod title_id;

use std::ops::RangeInclusive;

use schemars::JsonSchema;
use serde::Deserialize;

use crate::{Data, yaml::Binary};

pub use removal::{RemovalInput, RemovalName};
pub use sha256::Sha256;
pub use title_id::TitleId;

pub const WAD_SIZE: RangeInclusive<usize> = 0x40..=0x2100_0000;

/// WAD signature requirements.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum SignaturePolicy {
    /// Require Nintendo signatures.
    #[default]
    Nintendo,
    /// Permit an unsigned community channel after content validation.
    AllowUnsigned,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Title {
    pub source: Data<Binary>,
    /// SHA-256 of the exact WAD bytes.
    pub sha256: Sha256,
    #[serde(default)]
    pub signature_policy: SignaturePolicy,
}
