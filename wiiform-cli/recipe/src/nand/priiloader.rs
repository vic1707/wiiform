// SPDX-License-Identifier: GPL-2.0-only
//! Priiloader settings and System Menu hacks.
mod hack;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

pub use self::hack::{BackgroundMusic, DiscIos, Hack, UsbGecko};

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

/// Serde defaults for the keys a recipe leaves out, noted for the build log.
mod defaults {
    use super::{Autoboot, MountPoint, ReturnTo, ShutdownTo};
    use crate::defaults::{record, recorded};

    // Without rvloader, which sets them, the install uses the System Menu.
    pub(super) fn autoboot() -> Option<Autoboot> {
        record("nand.priiloader.autoboot", Autoboot::default());
        None
    }

    pub(super) fn return_to() -> Option<ReturnTo> {
        record("nand.priiloader.return_to", ReturnTo::default());
        None
    }

    recorded! {
        shutdown_to: ShutdownTo = "nand.priiloader.shutdown_to";
        preferred_mount_point: MountPoint = "nand.priiloader.preferred_mount_point";
        stop_disc: bool = "nand.priiloader.stop_disc";
        lid_slot_on_error: bool = "nand.priiloader.lid_slot_on_error";
        ignore_shutdown_mode: bool = "nand.priiloader.ignore_shutdown_mode";
        white_background: bool = "nand.priiloader.white_background";
        dump_gecko_text: bool = "nand.priiloader.dump_gecko_text";
        show_rc_updates: bool = "nand.priiloader.show_rc_updates";
    }
}

/// Complete Priiloader configuration. Applying it writes every setting and
/// hack state, replacing what the NAND holds; omitted values use the pinned
/// 0.10.0 defaults.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[serde(deny_unknown_fields)]
pub struct Priiloader {
    /// Autoboot destination. Defaults to the System Menu.
    #[cfg_attr(feature = "cli", arg(long, value_enum))]
    #[serde(default = "defaults::autoboot")]
    pub autoboot: Option<Autoboot>,
    /// Destination used by Priiloader's Return to option. Defaults to the System Menu.
    #[cfg_attr(feature = "cli", arg(long, value_enum))]
    #[serde(default = "defaults::return_to")]
    pub return_to: Option<ReturnTo>,
    /// Shutdown destination. Defaults to none.
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value_t))]
    #[serde(default = "defaults::shutdown_to")]
    pub shutdown_to: ShutdownTo,
    /// Preferred storage device. Defaults to automatic.
    #[cfg_attr(feature = "cli", arg(long, value_enum, default_value_t))]
    #[serde(default = "defaults::preferred_mount_point")]
    pub preferred_mount_point: MountPoint,
    /// Stop the disc drive at startup.
    #[cfg_attr(feature = "cli", arg(long))]
    #[serde(default = "defaults::stop_disc")]
    pub stop_disc: bool,
    /// Light the drive slot when an error occurs.
    #[cfg_attr(feature = "cli", arg(long))]
    #[serde(default = "defaults::lid_slot_on_error")]
    pub lid_slot_on_error: bool,
    /// Ignore standby mode.
    #[cfg_attr(feature = "cli", arg(long))]
    #[serde(default = "defaults::ignore_shutdown_mode")]
    pub ignore_shutdown_mode: bool,
    /// Use a white background instead of the default black one.
    #[cfg_attr(feature = "cli", arg(long))]
    #[serde(default = "defaults::white_background")]
    pub white_background: bool,
    /// Dump Gecko output.
    #[cfg_attr(feature = "cli", arg(long))]
    #[serde(default = "defaults::dump_gecko_text")]
    pub dump_gecko_text: bool,
    /// Show Priiloader update notices.
    #[cfg_attr(feature = "cli", arg(long))]
    #[serde(default = "defaults::show_rc_updates")]
    pub show_rc_updates: bool,
    /// Hacks to enable, each combining with any other. Every hack not listed is
    /// disabled. The background music, USB Gecko and force_disc_ios hacks have
    /// their own keys, since each group takes at most one hack.
    #[cfg_attr(feature = "cli", arg(long = "hack", value_enum, value_name = "HACK"))]
    #[schemars(extend("uniqueItems" = true), transform = crate::schema::list_values::<Hack>)]
    #[serde(default)]
    pub hacks: Vec<Hack>,
    /// The System Menu's background music hack, or none.
    #[cfg_attr(feature = "cli", arg(long, value_enum))]
    #[serde(default)]
    pub background_music: Option<BackgroundMusic>,
    /// Where the Wii Menu's logs go to a USB Gecko, or none.
    #[cfg_attr(feature = "cli", arg(long, value_enum))]
    #[serde(default)]
    pub usb_gecko: Option<UsbGecko>,
    /// The IOS that runs disc games, or none. The IOS must hold a cIOS.
    #[cfg_attr(feature = "cli", arg(long, value_enum))]
    #[serde(default)]
    pub force_disc_ios: Option<DiscIos>,
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
