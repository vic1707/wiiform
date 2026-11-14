// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// One list defines the recipe keys, CLI names, and native IDs.
macro_rules! priiloader_hacks {
    ($(#[doc = $doc:literal] $variant:ident = $code:literal => $field:ident),+ $(,)?) => {
        /// A hack supported by the pinned Priiloader definitions.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u8)]
        pub enum PriiloaderHack {
            $(
                #[doc = $doc]
                $variant = $code,
            )+
        }

        /// Complete supported System Menu hack state. Each hack defaults to off.
        #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
        #[cfg_attr(feature = "cli", derive(clap::Args))]
        #[serde(default, deny_unknown_fields)]
        pub struct HackSettings {
            $(
                #[doc = $doc]
                #[cfg_attr(feature = "cli", arg(long))]
                pub $field: bool,
            )+
        }

        impl HackSettings {
            /// Iterate over enabled hacks in native code order.
            pub fn enabled(&self) -> impl Iterator<Item = PriiloaderHack> {
                [$(self.$field.then_some(PriiloaderHack::$variant)),+].into_iter().flatten()
            }
        }
    };
}

priiloader_hacks! {
    /// Fix 480p graphics in the System Menu.
    Fix480p = 0 => fix_480p,
    /// Block online updates.
    BlockOnlineUpdates = 1 => block_online_updates,
    /// Block disc updates.
    BlockDiscUpdates = 2 => block_disc_updates,
    /// Block disc autoboot.
    BlockDiscAutoboot = 3 => block_disc_autoboot,
    /// Auto-press A at the health screen.
    AutoPressA = 4 => auto_press_a,
    /// Remove deflicker.
    RemoveDeflicker = 5 => remove_deflicker,
    /// Move the Disc Channel.
    MoveDiscChannel = 6 => move_disc_channel,
    /// Remove NoCopy save protection.
    RemoveNocopySaveProtection = 7 => remove_nocopy_save_protection,
    /// Replace the health screen with Backmenu.
    ReplaceHealthScreen = 8 => replace_health_screen,
    /// Disable System Menu background music.
    NoBackgroundMusic = 9 => no_background_music,
    /// Apply the Wiimmfi patch.
    Wiimmfi = 10 => wiimmfi,
    /// Enable Region Free EVERYTHING (Japanese menu uses the JP hack name).
    RegionFreeEverything = 11 => region_free_everything,
}
