// SPDX-License-Identifier: GPL-2.0-only
//! Priiloader assets, settings, and System Menu hacks.

use std::path::PathBuf;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Priiloader autoboot selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Autoboot {
    #[serde(rename = "disabled")]
    Disabled,
    #[serde(rename = "homebrew_channel")]
    HomebrewChannel,
    #[serde(rename = "bootmii_ios")]
    BootmiiIos,
    #[serde(rename = "system_menu")]
    SystemMenu,
    #[serde(rename = "file")]
    File,
}

/// Priiloader return destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum ReturnTo {
    #[serde(rename = "system_menu")]
    SystemMenu,
    #[serde(rename = "priiloader")]
    Priiloader,
    #[serde(rename = "autoboot")]
    Autoboot,
    #[serde(rename = "file")]
    File,
}

/// Priiloader shutdown destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum ShutdownTo {
    #[serde(rename = "none")]
    None,
    #[serde(rename = "priiloader")]
    Priiloader,
    #[serde(rename = "autoboot")]
    Autoboot,
}

/// Preferred storage device for Priiloader file operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum MountPoint {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "sd")]
    Sd,
    #[serde(rename = "usb")]
    Usb,
}

/// Priiloader settings written to its settings file.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PriiloaderSettings {
    #[schemars(with = "crate::schema::NonNull<Autoboot>")]
    pub autoboot: Option<Autoboot>,
    #[schemars(with = "crate::schema::NonNull<ReturnTo>")]
    pub return_to: Option<ReturnTo>,
    #[schemars(with = "crate::schema::NonNull<ShutdownTo>")]
    pub shutdown_to: Option<ShutdownTo>,
    #[schemars(with = "crate::schema::NonNull<MountPoint>")]
    pub preferred_mount_point: Option<MountPoint>,
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub stop_disc: Option<bool>,
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub lid_slot_on_error: Option<bool>,
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub ignore_shutdown_mode: Option<bool>,
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub black_background: Option<bool>,
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub dump_gecko_text: Option<bool>,
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub show_rc_updates: Option<bool>,
}

/// Priiloader hacks supported by the pinned definitions.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HackSettings {
    /// Fix 480p graphics in the System Menu.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub fix_480p: Option<bool>,
    /// Block online updates.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub block_online_updates: Option<bool>,
    /// Block disc updates.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub block_disc_updates: Option<bool>,
    /// Block disc autoboot.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub block_disc_autoboot: Option<bool>,
    /// Auto-press A at the health screen.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub auto_press_a: Option<bool>,
    /// Remove deflicker.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub remove_deflicker: Option<bool>,
    /// Move the Disc Channel.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub move_disc_channel: Option<bool>,
    /// Remove NoCopy save protection.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub remove_nocopy_save_protection: Option<bool>,
    /// Replace the health screen with Backmenu.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub replace_health_screen: Option<bool>,
    /// Disable System Menu background music.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub no_background_music: Option<bool>,
    /// Apply the Wiimmfi patch.
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub wiimmfi: Option<bool>,
    /// Enable Region Free EVERYTHING (Japanese menu uses the JP hack name).
    #[schemars(with = "crate::schema::NonNull<bool>")]
    pub region_free_everything: Option<bool>,
}

/// Priiloader installation and configuration.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PriiloaderFile {
    /// Optional prebuilt Priiloader installer app, relative to the recipe file.
    #[schemars(with = "crate::schema::NonNull<std::path::PathBuf>")]
    pub app: Option<PathBuf>,
    /// Optional settings overrides.
    #[serde(default)]
    pub settings: PriiloaderSettings,
    /// Optional exact-name hack toggles.
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
