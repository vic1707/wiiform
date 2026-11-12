// SPDX-License-Identifier: GPL-2.0-only
//! WAD requests, title identifiers, and channel removals.

mod removal;
mod sha256;
mod title_id;

pub use removal::{RemovalInput, RemovalName};
pub use sha256::Sha256;
pub use title_id::TitleId;

pub(crate) use removal::allowlisted_title_ids;

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// WAD signature requirements.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum SignaturePolicy {
    /// Require Nintendo signatures.
    #[default]
    #[serde(rename = "nintendo")]
    Nintendo,
    /// Permit an unsigned community channel WAD after content validation.
    #[serde(rename = "allow-unsigned")]
    AllowUnsigned,
}

/// A WAD installation selected by a recipe.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TitleFile {
    /// Local path, resolved relative to the recipe file.
    pub path: PathBuf,
    /// Required SHA-256 of the exact WAD file.
    pub sha256: Sha256,
    /// Signature policy; defaults to `nintendo`.
    #[serde(default)]
    pub signature_policy: SignaturePolicy,
}

/// A resolved title file passed from recipe parsing to the workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Title {
    pub path: PathBuf,
    pub sha256: Sha256,
    pub signature_policy: SignaturePolicy,
}
