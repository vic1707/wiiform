// SPDX-License-Identifier: GPL-2.0-only
//! WAD requests, title identifiers, and channel removals.

mod cios;
mod removal;
mod sha256;
mod title_id;
mod wad;

use schemars::JsonSchema;
use serde::Deserialize;

pub use cios::{Cios, CiosBase, CiosOptions, CiosSlot, CiosTarget};
pub use removal::{RemovalInput, RemovalName};
pub use sha256::Sha256;
pub use title_id::TitleId;

pub(crate) use wad::WAD_BOUNDS;
pub use wad::Wad;

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

pub type Title = Wad<TitleOptions>;

#[derive(Debug, Default, Deserialize, JsonSchema)]
#[schemars(rename = "Title")]
pub struct TitleOptions {
    #[serde(default)]
    pub signature_policy: SignaturePolicy,
}
