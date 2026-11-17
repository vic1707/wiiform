// SPDX-License-Identifier: GPL-2.0-only
//! WADs to install: the `titles` key.
use schemars::JsonSchema;
use serde::Deserialize;

use crate::Wad;

/// A WAD to install when its title is absent, or to verify when it is present.
// The flattened WAD rejects unknown keys for the whole entry: see `WadFile`.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct Title {
    #[serde(flatten)]
    pub wad: Wad,
    /// Permit an unsigned community channel after content validation.
    /// By default the WAD must carry Nintendo signatures.
    #[serde(default)]
    pub allow_unsigned: bool,
}
