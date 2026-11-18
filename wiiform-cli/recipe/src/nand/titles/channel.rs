// SPDX-License-Identifier: GPL-2.0-only
//! Channel WADs: the `titles.channels` key.
use schemars::JsonSchema;
use serde::Deserialize;

use crate::Wad;

/// A channel WAD to install when its title is absent, or to verify when it is
/// present: a channel, a Virtual Console or WiiWare title, or an app channel.
// The flattened WAD rejects unknown keys for the whole entry: see `WadFile`.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct Channel {
    #[serde(flatten)]
    pub wad: Wad,
    /// Permit an unsigned community channel after content validation.
    /// By default the WAD must carry Nintendo signatures.
    #[serde(default)]
    pub allow_unsigned: bool,
}
