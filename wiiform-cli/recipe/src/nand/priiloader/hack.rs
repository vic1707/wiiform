// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// A System Menu hack from the pinned Priiloader 0.10.0 definitions. Descriptions
/// follow Priiloader's docs/HACKSLIST.md.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
#[cfg_attr(feature = "cli", derive(clap::ValueEnum))]
#[cfg_attr(feature = "cli", value(rename_all = "snake_case"))]
#[serde(rename_all = "snake_case")]
pub enum Hack {
    /// Fixes an image display issue of the System Menu in 480p (analog video only).
    /// It fixes the System Menu, not games.
    #[serde(rename = "fix_480p")]
    #[cfg_attr(feature = "cli", value(name = "fix_480p"))]
    Fix480p,
    /// Makes System Menu updates fail (error 32007), to stay on this System Menu.
    BlockOnlineUpdates,
    /// Skips the "Wii System Update" some game discs force before playing.
    BlockDiscUpdates,
    /// Stops discs whose ID starts with 0 or 1 (diagnostic discs) from booting automatically.
    BlockDiscAutoboot,
    /// Presses A automatically to get past the health and safety screen.
    AutoPressA,
    /// Disables the System Menu's blur filter; recommended with 480p.
    RemoveDeflicker,
    /// Lets the Disc Channel be moved like any channel (hold A+B).
    MoveDiscChannel,
    /// Lets Data Management copy save files that are normally protected from copying
    /// to the SD card. It does not allow moving them.
    RemoveNocopySaveProtection,
    /// Skips the health warning screen and goes straight to the channels.
    ReplaceHealthScreen,
    /// Disables the System Menu's background music.
    NoBackgroundMusic,
    /// Patches disc games launched from the Disc Channel to use Wiimmfi (not WiiWare).
    Wiimmfi,
    /// Unblocks region-locked Wii and GameCube discs and channels (WiiWare, VC). Not every
    /// game works; video modes may misbehave. The Japanese menu uses the JP definition.
    RegionFreeEverything,
    /// Disables the System Menu's sound effects and background music.
    #[serde(rename = "no_sfx_and_background_music")]
    #[cfg_attr(feature = "cli", value(name = "no_sfx_and_background_music"))]
    NoSfxAndBackgroundMusic,
    /// Re-enables the Bannerbomb exploit on 4.3. Not needed with the Homebrew Channel.
    ReEnableBannerbomb,
    /// Sends the Wii Menu's logs to a USB Gecko in memory card slot B.
    UsbGeckoReport,
    /// Sends the Wii Menu's logs to a USB Gecko in slot B when Gecko OS launched the menu.
    #[serde(rename = "usb_gecko_report_geckoos")]
    #[cfg_attr(feature = "cli", value(name = "usb_gecko_report_geckoos"))]
    UsbGeckoReportGeckoOs,
    /// Boots straight into Data Management. Can help with a banner brick by deleting
    /// the channel data causing it.
    ForceDataManagement,
    /// Starts the console in recovery mode, to launch recovery discs.
    ForceRecoveryMode,
    /// In recovery mode, lets any disc boot instead of waiting for the Wii Backup Disc.
    RemoveDiagnosticDiscCheck,
    /// Prevents the System Menu from deleting the Homebrew Channel, DVDX and others
    /// (HAXX, JODI, DVDX, DISC, DISK, RZDx).
    NoDeleteHomebrew,
    /// Runs disc games under IOS248, for a cIOS installed there. Disc games without
    /// one there stop working.
    #[serde(rename = "force_disc_ios248")]
    #[cfg_attr(feature = "cli", value(name = "force_disc_ios248"))]
    ForceDiscIos248,
    /// Runs disc games under IOS249 (see force_disc_ios248).
    #[serde(rename = "force_disc_ios249")]
    #[cfg_attr(feature = "cli", value(name = "force_disc_ios249"))]
    ForceDiscIos249,
    /// Runs disc games under IOS250 (see force_disc_ios248).
    #[serde(rename = "force_disc_ios250")]
    #[cfg_attr(feature = "cli", value(name = "force_disc_ios250"))]
    ForceDiscIos250,
    /// Runs disc games under IOS251 (see force_disc_ios248).
    #[serde(rename = "force_disc_ios251")]
    #[cfg_attr(feature = "cli", value(name = "force_disc_ios251"))]
    ForceDiscIos251,
    /// Unblocks service discs: the Wii Startup Disc and the Wii Backup Discs.
    UnblockServiceDiscs,
    /// Stops the System Menu from blocking discs that use IOS16 (the Wii Backup Disc).
    #[serde(rename = "remove_ios16_disc_error")]
    #[cfg_attr(feature = "cli", value(name = "remove_ios16_disc_error"))]
    RemoveIos16DiscError,
    /// Marks the network connection as tested, so "Use this connection" works
    /// without running the test.
    MarkNetworkTested,
}

impl Hack {
    /// Hacks that cannot be enabled together: the second one in each pair either
    /// includes the first or stops it from working.
    pub(crate) const EXCLUSIVE: &[&[Self]] = &[
        &[Self::NoBackgroundMusic, Self::NoSfxAndBackgroundMusic],
        &[Self::UsbGeckoReport, Self::UsbGeckoReportGeckoOs],
        &[
            Self::ForceDiscIos248,
            Self::ForceDiscIos249,
            Self::ForceDiscIos250,
            Self::ForceDiscIos251,
        ],
    ];
}
