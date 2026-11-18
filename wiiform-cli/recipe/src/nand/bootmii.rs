// SPDX-License-Identifier: GPL-2.0-only
//! BootMii as IOS: the `bootmii` key.
use schemars::JsonSchema;
use serde::Deserialize;

/// Install BootMii as IOS (IOS254) from the seed, which holds it when its backup
/// was taken after the HackMii Installer put BootMii on the console. BootMii
/// itself runs from the SD card's /bootmii/ folder: add those files under `sd`.
/// The Homebrew Channel ("Launch BootMii") and Priiloader start it.
#[derive(Clone, Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Bootmii {}
