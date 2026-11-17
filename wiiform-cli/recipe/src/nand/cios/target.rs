// SPDX-License-Identifier: GPL-2.0-only
mod base;
mod slot;

use schemars::JsonSchema;
use serde::Deserialize;

pub use self::{base::Base, slot::Slot};
use crate::TitleId;

/// Expected identity and replacement policy for a prebuilt Wii d2x WAD.
#[derive(Debug, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[serde(deny_unknown_fields)]
pub struct Target {
    /// Destination IOS slot (248–251).
    #[cfg_attr(feature = "cli", arg(long))]
    pub slot: Slot,
    /// Base IOS declared by the d2x payload.
    #[cfg_attr(feature = "cli", arg(long))]
    pub base: Base,
    /// TMD title revision, such as 65535.
    #[cfg_attr(feature = "cli", arg(long))]
    pub revision: u16,
    /// Permit replacement of the verified Nintendo stub in slot 249 or 250.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub replace_stock_stub: bool,
}

impl Target {
    pub fn title_id(&self) -> TitleId {
        let slot: u8 = self.slot.into();
        TitleId::known(0x0000_0001_0000_0000 | u64::from(slot))
    }
}
