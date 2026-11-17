// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A System Menu hack from the pinned Priiloader definitions.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum Hack {
    /// Fix 480p graphics in the System Menu.
    #[serde(rename = "fix_480p")]
    #[cfg_attr(feature = "cli", value(name = "fix_480p"))]
    Fix480p,
    /// Block online updates.
    BlockOnlineUpdates,
    /// Block disc updates.
    BlockDiscUpdates,
    /// Block disc autoboot.
    BlockDiscAutoboot,
    /// Auto-press A at the health screen.
    AutoPressA,
    /// Remove deflicker.
    RemoveDeflicker,
    /// Move the Disc Channel.
    MoveDiscChannel,
    /// Remove NoCopy save protection.
    RemoveNocopySaveProtection,
    /// Replace the health screen with Backmenu.
    ReplaceHealthScreen,
    /// Disable System Menu background music.
    NoBackgroundMusic,
    /// Apply the Wiimmfi patch.
    Wiimmfi,
    /// Enable Region Free EVERYTHING (the Japanese menu uses the JP definition).
    RegionFreeEverything,
}
