// SPDX-License-Identifier: GPL-2.0-only
//! Titles to install, by kind: the `titles` key.
pub mod channel;
pub mod cios;
pub mod ios;

use schemars::JsonSchema;
use serde::Deserialize;

pub use self::{channel::Channel, cios::Cios, ios::Ios};

/// Titles to install on top of the stock menu.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Titles {
    /// Channels, Virtual Console and WiiWare titles and app channels, as WADs.
    #[serde(default)]
    pub channels: Vec<Channel>,
    /// Nintendo IOS, downloaded from NUS by slot and version.
    #[serde(default)]
    pub ios: Vec<Ios>,
    /// d2x cIOS, built from a d2x release on a NUS base IOS or given as a prebuilt WAD.
    #[serde(default)]
    pub cios: Vec<Cios>,
}
