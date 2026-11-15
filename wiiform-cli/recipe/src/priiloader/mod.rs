// SPDX-License-Identifier: GPL-2.0-only
//! Priiloader assets, settings, and System Menu hacks.

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};

mod hacks;
mod settings;

pub use hacks::{HackSettings, PriiloaderHack};
pub use settings::{Autoboot, MountPoint, PriiloaderSettings, ReturnTo, ShutdownTo};

/// Priiloader installation and configuration.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Priiloader {
    /// Complete settings. Omitted values use pinned Priiloader defaults.
    #[serde(deserialize_with = "null_to_default")]
    #[schemars(with = "Option<PriiloaderSettings>")]
    pub settings: PriiloaderSettings,
    /// Complete state for supported hacks. Unlisted hacks default to off.
    #[serde(deserialize_with = "null_to_default")]
    #[schemars(with = "Option<HackSettings>")]
    pub hacks: HackSettings,
}

fn null_to_default<'de, T, D>(deserializer: D) -> Result<T, D::Error>
where
    T: Deserialize<'de> + Default,
    D: Deserializer<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}
