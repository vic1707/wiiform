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

/// Priiloader settings written to its settings file.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[serde(deny_unknown_fields)]
pub struct PriiloaderSettings {
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<Autoboot>")]
    #[cfg_attr(feature = "cli", arg(long, value_enum))]
    pub autoboot: Option<Autoboot>,
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<ReturnTo>")]
    #[cfg_attr(feature = "cli", arg(long, value_enum))]
    pub return_to: Option<ReturnTo>,
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<ShutdownTo>")]
    #[cfg_attr(feature = "cli", arg(long, value_enum))]
    pub shutdown_to: Option<ShutdownTo>,
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<MountPoint>")]
    #[cfg_attr(feature = "cli", arg(long, value_enum))]
    pub preferred_mount_point: Option<MountPoint>,
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<bool>")]
    #[cfg_attr(feature = "cli", arg(long, value_parser = parse_on_off, value_name = "on|off"))]
    pub stop_disc: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<bool>")]
    #[cfg_attr(feature = "cli", arg(long, value_parser = parse_on_off, value_name = "on|off"))]
    pub lid_slot_on_error: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<bool>")]
    #[cfg_attr(feature = "cli", arg(long, value_parser = parse_on_off, value_name = "on|off"))]
    pub ignore_shutdown_mode: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<bool>")]
    #[cfg_attr(feature = "cli", arg(long, value_parser = parse_on_off, value_name = "on|off"))]
    pub black_background: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<bool>")]
    #[cfg_attr(feature = "cli", arg(long, value_parser = parse_on_off, value_name = "on|off"))]
    pub dump_gecko_text: Option<bool>,
    #[serde(
        default,
        deserialize_with = "crate::schema::non_null_option",
        skip_serializing_if = "Option::is_none"
    )]
    #[schemars(with = "crate::schema::NonNull<bool>")]
    #[cfg_attr(feature = "cli", arg(long, value_parser = parse_on_off, value_name = "on|off"))]
    pub show_rc_updates: Option<bool>,
}

#[cfg(feature = "cli")]
pub fn parse_on_off(value: &str) -> Result<bool, String> {
    match value {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err("expected on or off".to_owned()),
    }
}
