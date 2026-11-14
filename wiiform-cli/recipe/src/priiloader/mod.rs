// SPDX-License-Identifier: GPL-2.0-only
//! Priiloader assets, settings, and System Menu hacks.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod hacks;
mod settings;

pub use hacks::{HackSettings, PriiloaderHack};
pub use settings::{Autoboot, MountPoint, PriiloaderSettings, ReturnTo, ShutdownTo};

/// Priiloader installation and configuration.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Priiloader {
    /// Complete settings. This section is required; omitted fields use pinned Priiloader defaults.
    pub settings: PriiloaderSettings,
    /// Complete state for supported hacks. Unlisted hacks default to off.
    #[serde(default)]
    pub hacks: HackSettings,
}
