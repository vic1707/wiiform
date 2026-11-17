// SPDX-License-Identifier: GPL-2.0-only
//! Prebuilt d2x cIOS WADs: the `cios` key.
pub mod target;

use schemars::JsonSchema;
use serde::Deserialize;

pub use self::target::Target;
use crate::Wad;

/// A prebuilt Wii d2x cIOS WAD and the slot it is expected to fill.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct Cios {
    #[serde(flatten)]
    pub wad: Wad,
    pub target: Target,
}
