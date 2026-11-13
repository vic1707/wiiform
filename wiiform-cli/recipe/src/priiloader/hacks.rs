// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// One list defines the recipe keys, CLI names, and native IDs.
macro_rules! priiloader_hacks {
    ($(#[doc = $doc:literal] $variant:ident = $code:literal => $field:ident),+ $(,)?) => {
        /// A hack supported by the pinned Priiloader definitions.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
        #[repr(u8)]
        pub enum PriiloaderHack {
            $(
                #[doc = $doc]
                #[cfg_attr(feature = "cli", value(name = stringify!($field)))]
                $variant = $code,
            )+
        }

        impl PriiloaderHack {
            /// The stable recipe and CLI name.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => stringify!($field)),+
                }
            }
        }

        /// Explicit System Menu hack overrides.
        #[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub struct HackSettings {
            $(
                #[doc = $doc]
                #[serde(
                    default,
                    deserialize_with = "crate::schema::non_null_option",
                    skip_serializing_if = "Option::is_none"
                )]
                #[schemars(with = "crate::schema::NonNull<bool>")]
                pub $field: Option<bool>,
            )+
        }

        impl HackSettings {
            /// Set a hack once. Return false if the caller already specified it.
            pub fn set(&mut self, hack: PriiloaderHack, enabled: bool) -> bool {
                let field = match hack {
                    $(PriiloaderHack::$variant => &mut self.$field),+
                };
                if field.is_some() {
                    return false;
                }
                *field = Some(enabled);
                true
            }

            /// Iterate over the requested overrides in native code order.
            pub fn overrides(&self) -> impl Iterator<Item = (PriiloaderHack, bool)> {
                [$(self.$field.map(|enabled| (PriiloaderHack::$variant, enabled))),+]
                    .into_iter()
                    .flatten()
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
