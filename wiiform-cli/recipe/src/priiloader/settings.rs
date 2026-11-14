// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Priiloader autoboot selection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum Autoboot {
    Disabled = 0,
    HomebrewChannel = 1,
    BootmiiIos = 2,
    SystemMenu = 3,
    File = 4,
}

/// Priiloader return destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ReturnTo {
    SystemMenu = 0,
    Priiloader = 1,
    Autoboot = 2,
    File = 3,
}

/// Priiloader shutdown destination.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ShutdownTo {
    None = 0,
    Priiloader = 1,
    Autoboot = 2,
}

/// Preferred storage device for Priiloader file operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum MountPoint {
    Auto = 0,
    Sd = 1,
    Usb = 2,
}

/// Complete Priiloader settings written to its settings file.
///
/// Missing recipe values use the pinned 0.10.0 defaults. Applying this type
/// writes every configurable setting, replacing values already present in NAND.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[serde(default, deny_unknown_fields)]
pub struct PriiloaderSettings {
    /// Autoboot destination. Defaults to the System Menu.
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value = "system_menu"))]
    pub autoboot: Autoboot,
    /// Destination used by Priiloader's Return to option.
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value = "system_menu"))]
    pub return_to: ReturnTo,
    /// Shutdown destination. Defaults to none.
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value = "none"))]
    pub shutdown_to: ShutdownTo,
    /// Preferred storage device. Defaults to automatic.
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value = "auto"))]
    pub preferred_mount_point: MountPoint,
    /// Stop the disc drive at startup. Defaults to off.
    #[cfg_attr(feature = "cli", arg(long, action = clap::ArgAction::Set, value_parser = parse_on_off, default_value = "off", value_name = "on|off"))]
    pub stop_disc: bool,
    /// Light the drive slot when an error occurs. Defaults to off.
    #[cfg_attr(feature = "cli", arg(long, action = clap::ArgAction::Set, value_parser = parse_on_off, default_value = "off", value_name = "on|off"))]
    pub lid_slot_on_error: bool,
    /// Ignore standby mode. Defaults to off.
    #[cfg_attr(feature = "cli", arg(long, action = clap::ArgAction::Set, value_parser = parse_on_off, default_value = "off", value_name = "on|off"))]
    pub ignore_shutdown_mode: bool,
    /// Use a black background. The pinned default is on.
    #[cfg_attr(feature = "cli", arg(long, action = clap::ArgAction::Set, value_parser = parse_on_off, default_value = "on", value_name = "on|off"))]
    pub black_background: bool,
    /// Dump Gecko output. Defaults to off.
    #[cfg_attr(feature = "cli", arg(long, action = clap::ArgAction::Set, value_parser = parse_on_off, default_value = "off", value_name = "on|off"))]
    pub dump_gecko_text: bool,
    /// Show Priiloader update notices. Defaults to off.
    #[cfg_attr(feature = "cli", arg(long, action = clap::ArgAction::Set, value_parser = parse_on_off, default_value = "off", value_name = "on|off"))]
    pub show_rc_updates: bool,
}

impl Default for PriiloaderSettings {
    fn default() -> Self {
        Self {
            autoboot: Autoboot::SystemMenu,
            return_to: ReturnTo::SystemMenu,
            shutdown_to: ShutdownTo::None,
            preferred_mount_point: MountPoint::Auto,
            stop_disc: false,
            lid_slot_on_error: false,
            ignore_shutdown_mode: false,
            black_background: true,
            dump_gecko_text: false,
            show_rc_updates: false,
        }
    }
}

#[cfg(feature = "cli")]
fn parse_on_off(value: &str) -> Result<bool, String> {
    match value {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err("expected on or off".to_owned()),
    }
}
