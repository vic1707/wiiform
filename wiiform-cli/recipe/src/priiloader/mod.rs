// SPDX-License-Identifier: GPL-2.0-only
//! Priiloader assets, settings, and System Menu hacks.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

mod hacks;
mod settings;

pub use hacks::{HackSettings, PriiloaderHack};
#[cfg(feature = "cli")]
pub use settings::parse_on_off;
pub use settings::{Autoboot, MountPoint, PriiloaderSettings, ReturnTo, ShutdownTo};

/// Priiloader installation and configuration.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PriiloaderFile {
    /// Optional prebuilt Priiloader installer app, relative to the recipe file.
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<std::path::PathBuf>")]
    pub app: Option<PathBuf>,
    /// Optional settings overrides.
    #[serde(default)]
    pub settings: PriiloaderSettings,
    /// Optional exact-name hack toggles.
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<HackSettings>")]
    pub hacks: Option<HackSettings>,
}

/// Resolved Priiloader file, settings and hack values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Priiloader {
    pub app: Option<PathBuf>,
    pub settings: PriiloaderSettings,
    pub hacks: Option<HackSettings>,
}
