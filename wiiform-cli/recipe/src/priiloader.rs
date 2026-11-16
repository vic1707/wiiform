// SPDX-License-Identifier: GPL-2.0-only
//! Priiloader settings and System Menu hacks.
mod hack;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use self::hack::Hack;

/// Declares an option enum that the recipe and the command line spell the same way.
macro_rules! options {
    ($(#[doc = $doc:literal] $name:ident { $($variants:tt)+ })+) => {$(
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd)]
        #[derive(Serialize, Deserialize, JsonSchema)]
        #[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
        #[cfg_attr(feature = "cli", value(rename_all = "snake_case"))]
        #[serde(rename_all = "snake_case")]
        pub enum $name { $($variants)+ }
    )+};
}

options! {
    /// Priiloader autoboot selection.
    Autoboot { Disabled, HomebrewChannel, BootmiiIos, #[default] SystemMenu, File }
    /// Priiloader return destination.
    ReturnTo { #[default] SystemMenu, Priiloader, Autoboot, File }
    /// Priiloader shutdown destination.
    ShutdownTo { #[default] None, Priiloader, Autoboot }
    /// Preferred storage device for Priiloader file operations.
    MountPoint { #[default] Auto, Sd, Usb }
}

/// Complete Priiloader configuration. Applying it writes every setting and
/// hack state, replacing what the NAND holds; omitted values use the pinned
/// 0.10.0 defaults.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[serde(default, deny_unknown_fields)]
pub struct Priiloader {
    /// Autoboot destination. Defaults to the System Menu.
    #[cfg_attr(feature = "cli", arg(long, value_enum))]
    pub autoboot: Option<Autoboot>,
    /// Destination used by Priiloader's Return to option. Defaults to the System Menu.
    #[cfg_attr(feature = "cli", arg(long, value_enum))]
    pub return_to: Option<ReturnTo>,
    /// Shutdown destination. Defaults to none.
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value_t))]
    pub shutdown_to: ShutdownTo,
    /// Preferred storage device. Defaults to automatic.
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value_t))]
    pub preferred_mount_point: MountPoint,
    /// Stop the disc drive at startup.
    #[cfg_attr(feature = "cli", arg(long))]
    pub stop_disc: bool,
    /// Light the drive slot when an error occurs.
    #[cfg_attr(feature = "cli", arg(long))]
    pub lid_slot_on_error: bool,
    /// Ignore standby mode.
    #[cfg_attr(feature = "cli", arg(long))]
    pub ignore_shutdown_mode: bool,
    /// Use a white background instead of the default black one.
    #[cfg_attr(feature = "cli", arg(long))]
    pub white_background: bool,
    /// Dump Gecko output.
    #[cfg_attr(feature = "cli", arg(long))]
    pub dump_gecko_text: bool,
    /// Show Priiloader update notices.
    #[cfg_attr(feature = "cli", arg(long))]
    pub show_rc_updates: bool,
    /// Hacks to enable. Every other hack is disabled.
    #[cfg_attr(feature = "cli", arg(long = "hack", value_enum, value_name = "HACK"))]
    #[schemars(extend("uniqueItems" = true))]
    pub hacks: Vec<Hack>,
}

impl Priiloader {
    /// These settings, starting the executable stored as Priiloader's boot file
    /// at power-on and whenever a program returns.
    pub fn booting_file(&self) -> Self {
        Self {
            autoboot: Some(Autoboot::File),
            return_to: Some(ReturnTo::Autoboot),
            ..self.clone()
        }
    }
}
