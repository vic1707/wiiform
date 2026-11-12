// SPDX-License-Identifier: GPL-2.0-only
use std::{borrow::Cow, collections::BTreeSet, path::PathBuf};

use nnn::nnn;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Stock System Menu version used by the generated image.
#[derive(
    Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, JsonSchema,
)]
pub enum Menu {
    /// European 4.3 menu.
    #[serde(rename = "4.3E")]
    E43,
    /// North American 4.3 menu.
    #[serde(rename = "4.3U")]
    U43,
    /// Japanese 4.3 menu.
    #[serde(rename = "4.3J")]
    J43,
}

impl Menu {
    pub fn suffix(self) -> &'static str {
        match self {
            Self::E43 => "50",
            Self::U43 => "45",
            Self::J43 => "4a",
        }
    }

    pub fn allows_language(self, language: Language) -> bool {
        match self {
            Self::E43 => matches!(
                language,
                Language::En
                    | Language::De
                    | Language::Fr
                    | Language::Es
                    | Language::It
                    | Language::Nl
            ),
            Self::U43 => matches!(language, Language::En | Language::Fr | Language::Es),
            Self::J43 => language == Language::Ja,
        }
    }
}

#[nnn(
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Serialize,
        Deserialize
    ),
    nnn_derive(TryFrom, AsRef, Deref),
    sanitizers(lowercase),
    validators(regex = r"^[0-9a-f]{64}$")
)]
pub struct Sha256(String);

impl JsonSchema for Sha256 {
    fn schema_name() -> Cow<'static, str> {
        "Sha256".into()
    }
    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::Sha256").into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 64,
            "maxLength": 64,
            "pattern": "^[0-9a-fA-F]{64}$",
            "description": "64-character hexadecimal SHA-256 digest; normalized to lowercase"
        })
    }
}

#[nnn(
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Serialize,
        Deserialize
    ),
    nnn_derive(TryFrom, AsRef, Deref),
    sanitizers(lowercase),
    validators(regex = r"^[0-9a-f]{16}$")
)]
pub struct TitleId(String);

impl JsonSchema for TitleId {
    fn schema_name() -> Cow<'static, str> {
        "TitleId".into()
    }
    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::TitleId").into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 16,
            "maxLength": 16,
            "pattern": "^[0-9a-fA-F]{16}$",
            "description": "16-character hexadecimal Wii title ID; normalized to lowercase"
        })
    }
}

impl TitleId {
    pub fn as_str(&self) -> &str {
        self.as_ref().as_str()
    }
}

/// Channel shortcuts supported by `remove`.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum RemovalName {
    /// Expand to the forecast channel title for the selected menu and its common stub.
    #[serde(rename = "forecast-channel")]
    ForecastChannel,
    /// Expand to the news channel title for the selected menu and its common stub.
    #[serde(rename = "news-channel")]
    NewsChannel,
    /// Wii Shop Channel (HABA).
    #[serde(rename = "wii-shop-channel")]
    WiiShopChannel,
    /// Photo Channel versions HAAA and HAYA.
    #[serde(rename = "photo-channel")]
    PhotoChannel,
}

/// One removal entry as written in a recipe: a known name or quoted title ID.
#[derive(Clone, Debug, Eq, PartialEq, Deserialize, JsonSchema)]
#[serde(untagged)]
pub enum RemovalInput {
    /// Supported channel name.
    Name(RemovalName),
    /// A quoted title ID from the removal allowlist.
    Id(TitleId),
}

/// Allowed channels and local unsigned WAD policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum SignaturePolicy {
    /// Require Nintendo signatures.
    #[default]
    #[serde(rename = "nintendo")]
    Nintendo,
    /// Permit an unsigned community channel WAD after content validation.
    #[serde(rename = "allow-unsigned")]
    AllowUnsigned,
}

/// A WAD installation selected by a recipe.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct TitleFile {
    /// Local path, resolved relative to the recipe file.
    pub path: PathBuf,
    /// Required SHA-256 of the exact WAD file.
    pub sha256: Sha256,
    /// Signature policy; defaults to `nintendo`.
    #[serde(default)]
    pub signature_policy: SignaturePolicy,
}

/// Wii language selected in the first-run settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Language {
    #[serde(rename = "ja")]
    Ja,
    #[serde(rename = "en")]
    En,
    #[serde(rename = "de")]
    De,
    #[serde(rename = "fr")]
    Fr,
    #[serde(rename = "es")]
    Es,
    #[serde(rename = "it")]
    It,
    #[serde(rename = "nl")]
    Nl,
}

/// First-run setup fields written to SYSCONF.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    /// Optional console nickname (1–10 printable BMP characters).
    #[schemars(
        with = "crate::recipe::schema::NonNull<String>",
        length(min = 1, max = 10)
    )]
    pub nickname: Option<String>,
    /// Optional setup language, constrained by the effective menu region.
    #[schemars(with = "crate::recipe::schema::NonNull<Language>")]
    pub language: Option<Language>,
}

/// Screen aspect ratio.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
pub enum Aspect {
    #[serde(rename = "4:3")]
    FourThree,
    #[serde(rename = "16:9")]
    SixteenNine,
}

/// Display configuration written to SYSCONF.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Display {
    /// Set 4:3 or widescreen output.
    #[schemars(with = "crate::recipe::schema::NonNull<Aspect>")]
    pub aspect: Option<Aspect>,
    /// Enable progressive 480p output when supported by the display chain.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub progressive: Option<bool>,
    /// Enable PAL 60 Hz mode.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub pal60: Option<bool>,
}

impl Display {
    pub(crate) fn is_empty(&self) -> bool {
        self.aspect.is_none() && self.progressive.is_none() && self.pal60.is_none()
    }
}

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
    #[schemars(with = "crate::recipe::schema::NonNull<Autoboot>")]
    pub autoboot: Option<Autoboot>,
    #[schemars(with = "crate::recipe::schema::NonNull<ReturnTo>")]
    pub return_to: Option<ReturnTo>,
    #[schemars(with = "crate::recipe::schema::NonNull<ShutdownTo>")]
    pub shutdown_to: Option<ShutdownTo>,
    #[schemars(with = "crate::recipe::schema::NonNull<MountPoint>")]
    pub preferred_mount_point: Option<MountPoint>,
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub stop_disc: Option<bool>,
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub lid_slot_on_error: Option<bool>,
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub ignore_shutdown_mode: Option<bool>,
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub black_background: Option<bool>,
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub dump_gecko_text: Option<bool>,
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub show_rc_updates: Option<bool>,
}

/// Priiloader hacks supported by the pinned definitions.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct HackSettings {
    /// Fix 480p graphics in the System Menu.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub fix_480p: Option<bool>,
    /// Block online updates.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub block_online_updates: Option<bool>,
    /// Block disc updates.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub block_disc_updates: Option<bool>,
    /// Block disc autoboot.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub block_disc_autoboot: Option<bool>,
    /// Auto-press A at the health screen.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub auto_press_a: Option<bool>,
    /// Remove deflicker.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub remove_deflicker: Option<bool>,
    /// Move the Disc Channel.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub move_disc_channel: Option<bool>,
    /// Remove NoCopy save protection.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub remove_nocopy_save_protection: Option<bool>,
    /// Replace the health screen with Backmenu.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub replace_health_screen: Option<bool>,
    /// Disable System Menu background music.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub no_background_music: Option<bool>,
    /// Apply the Wiimmfi patch.
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub wiimmfi: Option<bool>,
    /// Enable Region Free EVERYTHING (Japanese menu uses the JP hack name).
    #[schemars(with = "crate::recipe::schema::NonNull<bool>")]
    pub region_free_everything: Option<bool>,
}

/// Priiloader installation and configuration.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PriiloaderFile {
    /// Optional prebuilt Priiloader installer app, relative to the recipe file.
    #[schemars(with = "crate::recipe::schema::NonNull<std::path::PathBuf>")]
    pub app: Option<PathBuf>,
    /// Optional settings overrides.
    #[serde(default)]
    pub settings: PriiloaderSettings,
    /// Optional exact-name hack toggles.
    #[schemars(with = "crate::recipe::schema::NonNull<HackSettings>")]
    pub hacks: Option<HackSettings>,
}

/// Recipe file shape, before resolving local paths and channel aliases.
#[derive(Clone, Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RecipeFile {
    /// Stock System Menu target (4.3E, 4.3U, or 4.3J).
    pub menu: Menu,
    /// Channels to remove; names expand using the effective menu's region.
    #[serde(default)]
    pub remove: Vec<RemovalInput>,
    /// Optional first-run setup values.
    #[schemars(with = "crate::recipe::schema::NonNull<Setup>")]
    pub setup: Option<Setup>,
    /// Optional display settings; at least one field is required when present.
    #[schemars(with = "crate::recipe::schema::NonNull<Display>")]
    pub display: Option<Display>,
    /// Optional WAD titles. Every title requires a path and SHA-256.
    #[serde(default)]
    pub titles: Vec<TitleFile>,
    /// Optional Priiloader install/settings/hacks.
    #[schemars(with = "crate::recipe::schema::NonNull<PriiloaderFile>")]
    pub priiloader: Option<PriiloaderFile>,
}

/// A resolved title file passed from recipe parsing to the workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Title {
    pub path: PathBuf,
    pub sha256: Sha256,
    pub signature_policy: SignaturePolicy,
}

/// Fully validated recipe consumed by the build workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Recipe {
    pub menu: Menu,
    /// Canonical allowlisted IDs after alias expansion for `menu`.
    pub remove: Vec<TitleId>,
    pub setup: Option<Setup>,
    pub display: Option<Display>,
    pub titles: Vec<Title>,
    pub priiloader: Option<Priiloader>,
}

/// Resolved Priiloader file, settings and hack values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Priiloader {
    pub app: Option<PathBuf>,
    pub settings: PriiloaderSettings,
    pub hacks: Option<HackSettings>,
}

const REGIONAL_CHANNELS: [(&str, &str); 2] = [
    ("forecast-channel", "00010002484146"),
    ("news-channel", "00010002484147"),
];
const COMMON_CHANNELS: [(&str, &[&str]); 2] = [
    ("wii-shop-channel", &["0001000248414241"]),
    ("photo-channel", &["0001000248414141", "0001000248415941"]),
];

impl RemovalInput {
    pub(crate) fn expand(&self, menu: Menu) -> Vec<TitleId> {
        match self {
            Self::Name(RemovalName::ForecastChannel) => {
                vec![
                    title_id(&format!("{}{}", REGIONAL_CHANNELS[0].1, menu.suffix())),
                    title_id(&format!("{}41", REGIONAL_CHANNELS[0].1)),
                ]
            }
            Self::Name(RemovalName::NewsChannel) => {
                vec![
                    title_id(&format!("{}{}", REGIONAL_CHANNELS[1].1, menu.suffix())),
                    title_id(&format!("{}41", REGIONAL_CHANNELS[1].1)),
                ]
            }
            Self::Name(RemovalName::WiiShopChannel) => vec![title_id(COMMON_CHANNELS[0].1[0])],
            Self::Name(RemovalName::PhotoChannel) => {
                COMMON_CHANNELS[1].1.iter().map(|id| title_id(id)).collect()
            }
            Self::Id(id) => vec![id.clone()],
        }
    }

    pub(crate) fn is_allowed(&self) -> bool {
        match self {
            Self::Name(_) => true,
            Self::Id(id) => allowlisted_title_ids().contains(id.as_str()),
        }
    }
}

pub(crate) fn allowlisted_title_ids() -> BTreeSet<&'static str> {
    let mut result = BTreeSet::new();
    for (_, base) in REGIONAL_CHANNELS {
        for suffix in ["50", "45", "4a", "41"] {
            result.insert(match suffix {
                "50" => {
                    if base.ends_with("46") {
                        "0001000248414650"
                    } else {
                        "0001000248414750"
                    }
                }
                "45" => {
                    if base.ends_with("46") {
                        "0001000248414645"
                    } else {
                        "0001000248414745"
                    }
                }
                "4a" => {
                    if base.ends_with("46") {
                        "000100024841464a"
                    } else {
                        "000100024841474a"
                    }
                }
                _ => {
                    if base.ends_with("46") {
                        "0001000248414641"
                    } else {
                        "0001000248414741"
                    }
                }
            });
        }
    }
    for (_, ids) in COMMON_CHANNELS {
        result.extend(ids.iter().copied());
    }
    result
}

pub(crate) fn parse_title_id(value: &str) -> TitleId {
    TitleId::try_from(value.to_owned()).expect("internal allowlisted title ID must be valid")
}

fn title_id(value: &str) -> TitleId {
    parse_title_id(value)
}
