// SPDX-License-Identifier: GPL-2.0-only
//! RVLoader's settings: its own `/rvloader/config.cfg`, and each game's
//! `/rvloader/configs/<ID>.cfg`. Both hold one `Key = value` line per setting.
//! RVLoader reads a missing key as its default, so only the keys given are
//! written.
//!
//! The keys and values come from RVLoader 2.1 (commit 3d950f3): the menus in
//! `gamesview.lua`, the defaults in `guigamesview.cpp`, and the enums in
//! `common/include/hiidratypes.h`, `main/include/peripherals/wifi.h` and
//! `main/include/nintendont.h`. Which keys a game's menu shows depends on its
//! type, and RVLoader ignores the rest, so a key can be set on a game whose
//! menu lacks it without any error.
use std::{collections::BTreeMap, fmt};

use anyhow::{Context, Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

use super::CardPath;

/// A game's ID, as RVLoader names the game's config file: the code in the game's
/// title, four to six upper-case letters or digits.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, JsonSchema)]
#[serde(try_from = "String")]
#[schemars(with = "String", extend("pattern" = "^[A-Z0-9]{4,6}$"))]
pub(super) struct RvloaderGameId(String);

impl TryFrom<String> for RvloaderGameId {
    type Error = anyhow::Error;

    fn try_from(text: String) -> Result<Self> {
        ensure!(
            (4..=6).contains(&text.len())
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit()),
            "{text} is not a game ID: use the four to six upper-case letters and digits in the game's title, such as RHAP01"
        );
        Ok(Self(text))
    }
}

impl fmt::Display for RvloaderGameId {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Wi-Fi for the game (`Enable WiFi`).
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum RvloaderWifi {
    Off,
    On,
    Auto,
}

impl RvloaderWifi {
    fn value(self) -> i64 {
        match self {
            Self::Off => 0,
            Self::On => 1,
            Self::Auto => 2,
        }
    }
}

/// Pad reading (`PADRead Mode`).
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum RvloaderPadRead {
    Auto,
    Bypass,
    Redirect,
}

impl RvloaderPadRead {
    fn value(self) -> i64 {
        match self {
            Self::Auto => 0,
            Self::Bypass => 1,
            Self::Redirect => 2,
        }
    }
}

/// Deflicker filter (`Deflicker Mode`).
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RvloaderDeflicker {
    Auto,
    Off,
    OffExtended,
    OnLow,
    OnMedium,
    OnHigh,
}

impl RvloaderDeflicker {
    fn value(self) -> i64 {
        match self {
            Self::Auto => 0,
            Self::Off => 1,
            Self::OffExtended => 2,
            Self::OnLow => 3,
            Self::OnMedium => 4,
            Self::OnHigh => 5,
        }
    }
}

/// Where cheats hook into the game (`Cheats Hooktype`).
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum RvloaderHooktype {
    Vbi,
    Kpadread,
    Joypad,
    Gxdraw,
    Gxflush,
    Ossleepthread,
    Axnextframe,
}

impl RvloaderHooktype {
    fn value(self) -> i64 {
        match self {
            Self::Vbi => 1,
            Self::Kpadread => 2,
            Self::Joypad => 3,
            Self::Gxdraw => 4,
            Self::Gxflush => 5,
            Self::Ossleepthread => 6,
            Self::Axnextframe => 7,
        }
    }
}

/// GameCube video mode (`Video mode`), as Nintendont's force flags encode it.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum RvloaderGcVideo {
    Auto,
    Pal50,
    Pal60,
    Ntsc,
    Mpal,
}

impl RvloaderGcVideo {
    fn value(self) -> i64 {
        // NIN_VID_FORCE (1 << 16) with the bit of each mode.
        match self {
            Self::Auto => 0,
            Self::Pal50 => 0x1_0001,
            Self::Pal60 => 0x1_0002,
            Self::Ntsc => 0x1_0004,
            Self::Mpal => 0x1_0008,
        }
    }
}

/// GameCube language (`Language`).
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
enum RvloaderLanguage {
    Auto,
    English,
    German,
    French,
    Spanish,
    Italian,
    Dutch,
}

impl RvloaderLanguage {
    fn value(self) -> i64 {
        match self {
            Self::Auto => -1,
            Self::English => 0,
            Self::German => 1,
            Self::French => 2,
            Self::Spanish => 3,
            Self::Italian => 4,
            Self::Dutch => 5,
        }
    }
}

fn flag(on: bool) -> i64 {
    i64::from(on)
}

/// One game's settings. A key left out keeps RVLoader's default for the game.
///
/// Wii discs (`wbfs`), VC and channels share the Wii keys; `usb_saves` is a Wii
/// disc option only; the GameCube keys apply to GameCube games.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("minProperties" = 1))]
pub(super) struct RvloaderGame {
    /// Wi-Fi: auto, on or off. Wii discs, VC and channels.
    wifi: Option<RvloaderWifi>,
    /// Bluetooth (a Wiimote and other Bluetooth controllers). Wii discs, VC and channels.
    bluetooth: Option<bool>,
    /// Saves on the USB drive instead of the NAND. Wii discs only.
    usb_saves: Option<bool>,
    /// GC2Wiimote: a Wiimote as a GameCube controller. Wii discs, VC and channels.
    gc2wiimote: Option<bool>,
    /// Patch the MX chip. Wii discs, VC and channels.
    patch_mx_chip: Option<bool>,
    /// Pad reading. Wii discs, VC and channels.
    padread_mode: Option<RvloaderPadRead>,
    /// Deflicker filter. Wii discs, VC and channels.
    deflicker_mode: Option<RvloaderDeflicker>,
    /// Cheats on. Wii discs, VC, channels and GameCube games.
    cheats: Option<bool>,
    /// Where cheats hook into the game. Wii discs, VC and channels.
    cheats_hooktype: Option<RvloaderHooktype>,
    /// Force progressive scan. GameCube games.
    force_progressive: Option<bool>,
    /// Force widescreen. GameCube games.
    force_widescreen: Option<bool>,
    /// Force RVL-DD stretching. GameCube games.
    force_rvl_dd_stretching: Option<bool>,
    /// Native SI. GameCube games.
    native_si: Option<bool>,
    /// Video mode. GameCube games.
    video_mode: Option<RvloaderGcVideo>,
    /// Language. GameCube games.
    language: Option<RvloaderLanguage>,
    /// Memory card emulation. GameCube games.
    memory_card_emulation: Option<bool>,
    /// Pads, 1 to 4. GameCube games.
    max_pads: Option<u8>,
    /// Video width: 0 for auto, otherwise 640 to 720. GameCube games.
    video_width: Option<u16>,
}

impl RvloaderGame {
    /// The file keys and values this game sets, in the order the menus list them.
    fn settings(&self) -> Result<Vec<(&'static str, i64)>> {
        let mut settings = Vec::new();
        put(
            &mut settings,
            "Enable_WiFi",
            self.wifi.map(RvloaderWifi::value),
        );
        put(&mut settings, "Enable_Bluetooth", self.bluetooth.map(flag));
        put(&mut settings, "Enable_USB_saves", self.usb_saves.map(flag));
        put(
            &mut settings,
            "Enable_GC2Wiimote",
            self.gc2wiimote.map(flag),
        );
        put(&mut settings, "Patch_MX_chip", self.patch_mx_chip.map(flag));
        put(
            &mut settings,
            "PADRead_Mode",
            self.padread_mode.map(RvloaderPadRead::value),
        );
        put(
            &mut settings,
            "Deflicker_Mode",
            self.deflicker_mode.map(RvloaderDeflicker::value),
        );
        put(&mut settings, "Enable_Cheats", self.cheats.map(flag));
        put(
            &mut settings,
            "Cheats_Hooktype",
            self.cheats_hooktype.map(RvloaderHooktype::value),
        );
        put(
            &mut settings,
            "Force_progressive",
            self.force_progressive.map(flag),
        );
        put(
            &mut settings,
            "Force_widescreen",
            self.force_widescreen.map(flag),
        );
        put(
            &mut settings,
            "Force_RVL_DD_stretching",
            self.force_rvl_dd_stretching.map(flag),
        );
        put(&mut settings, "Native_SI", self.native_si.map(flag));
        put(
            &mut settings,
            "Video_mode",
            self.video_mode.map(RvloaderGcVideo::value),
        );
        put(
            &mut settings,
            "Language",
            self.language.map(RvloaderLanguage::value),
        );
        put(
            &mut settings,
            "Memory_Card_Emulation",
            self.memory_card_emulation.map(flag),
        );
        if let Some(pads) = self.max_pads {
            ensure!((1..=4).contains(&pads), "max_pads is 1 to 4, not {pads}");
        }
        put(&mut settings, "Max_Pads", self.max_pads.map(i64::from));
        if let Some(width) = self.video_width {
            ensure!(
                width == 0 || (640..=720).contains(&width),
                "video_width is 0 (auto) or 640 to 720, not {width}"
            );
        }
        put(
            &mut settings,
            "Video_Width",
            self.video_width.map(i64::from),
        );
        Ok(settings)
    }
}

fn put(settings: &mut Vec<(&'static str, i64)>, key: &'static str, value: Option<i64>) {
    if let Some(value) = value {
        settings.push((key, value));
    }
}

/// A theme's folder name under `/rvloader/themes`.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(try_from = "String")]
#[schemars(with = "String", extend("pattern" = "^[A-Za-z0-9_-]{1,32}$"))]
pub(super) struct ThemeName(String);

impl TryFrom<String> for ThemeName {
    type Error = anyhow::Error;

    fn try_from(text: String) -> Result<Self> {
        ensure!(
            (1..=32).contains(&text.len())
                && text
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'),
            "{text} is not a theme folder name: use letters, digits, - and _"
        );
        Ok(Self(text))
    }
}

impl fmt::Display for ThemeName {
    fn fmt(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// The screen shown while a Wii game loads (`WiiLoadScreen`).
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum WiiLoadScreen {
    Verbose,
    SpinningDisc,
}

impl WiiLoadScreen {
    fn value(self) -> i64 {
        match self {
            Self::Verbose => 0,
            Self::SpinningDisc => 1,
        }
    }
}

/// RVLoader's own settings, written to `/rvloader/config.cfg`. A key left out
/// keeps RVLoader's default.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(extend("minProperties" = 1))]
pub(super) struct RvloaderConfig {
    /// Theme folder under /rvloader/themes, such as main. The theme's files must
    /// be on the drive: put them there with a bundle.
    theme: Option<ThemeName>,
    /// The load screen while a Wii game starts. Defaults to verbose.
    wii_load_screen: Option<WiiLoadScreen>,
}

impl RvloaderConfig {
    /// The file's lines, in RVLoader's `Key = value` form.
    fn lines(&self) -> Vec<String> {
        let mut lines = Vec::new();
        if let Some(theme) = &self.theme {
            lines.push(format!("theme = \"{theme}\""));
        }
        if let Some(screen) = self.wii_load_screen {
            lines.push(format!("WiiLoadScreen = {}", screen.value()));
        }
        lines
    }
}

/// Lines of an RVLoader config file, each ending in a newline.
fn file_text(lines: &[String]) -> Vec<u8> {
    lines
        .iter()
        .map(|line| format!("{line}\n"))
        .collect::<String>()
        .into_bytes()
}

/// RVLoader's settings: its own config, and each game's.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
#[schemars(transform = settings_rules)]
pub(super) struct RvloaderSettings {
    /// RVLoader's own settings, written to /rvloader/config.cfg.
    #[serde(default)]
    config: Option<RvloaderConfig>,
    /// Each game's settings, keyed by the game's ID, written to
    /// /rvloader/configs/<ID>.cfg.
    #[serde(default)]
    #[schemars(extend("minProperties" = 1))]
    games: Option<BTreeMap<RvloaderGameId, RvloaderGame>>,
}

/// The config, the games, or both: the settings need one of them.
fn settings_rules(schema: &mut schemars::Schema) {
    schema.insert(
        "anyOf".into(),
        serde_json::json!([{"required": ["config"], "properties": {"config": {"type": "object"}}}, {"required": ["games"], "properties": {"games": {"type": "object"}}}]),
    );
}

impl RvloaderSettings {
    /// The files the settings write: the config, then one file per game.
    pub(super) fn files(&self) -> Result<Vec<(CardPath, Vec<u8>)>> {
        ensure!(
            self.config.is_some() || self.games.is_some(),
            "rvloader sets neither config nor games"
        );
        let mut files = Vec::new();
        if let Some(config) = &self.config {
            let lines = config.lines();
            ensure!(!lines.is_empty(), "rvloader config sets no key");
            let path = CardPath::try_from("/rvloader/config.cfg".to_owned())?;
            files.push((path, file_text(&lines)));
        }
        if let Some(games) = &self.games {
            ensure!(!games.is_empty(), "rvloader lists no games");
            for (id, game) in games {
                let settings = game
                    .settings()
                    .with_context(|| format!("rvloader game {id}"))?;
                ensure!(!settings.is_empty(), "rvloader game {id} sets no key");
                let lines: Vec<String> = settings
                    .iter()
                    .map(|(key, value)| format!("{key} = {value}"))
                    .collect();
                let path = CardPath::try_from(format!("/rvloader/configs/{id}.cfg"))?;
                files.push((path, file_text(&lines)));
            }
        }
        Ok(files)
    }

    /// The folder the config's theme needs on the drive, if it names one.
    pub(super) fn folders(&self) -> Result<Vec<CardPath>> {
        let theme = self
            .config
            .as_ref()
            .and_then(|config| config.theme.as_ref());
        Ok(match theme {
            Some(theme) => vec![CardPath::try_from(format!("/rvloader/themes/{theme}"))?],
            None => Vec::new(),
        })
    }
}
