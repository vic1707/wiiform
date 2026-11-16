// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::Deserialize;

use super::{TitleId, Wad};

/// Expected identity and replacement policy for a prebuilt Wii d2x WAD.
#[derive(Debug, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[serde(deny_unknown_fields)]
pub struct CiosTarget {
    /// Destination IOS slot (248–251).
    #[schemars(range(min = 248, max = 251))]
    #[cfg_attr(feature = "cli", arg(long, value_parser = clap::value_parser!(u8).range(248..=251)))]
    pub slot: u8,
    /// Base IOS declared by the d2x payload.
    #[schemars(extend("enum" = [38, 56, 57, 58]))]
    #[cfg_attr(feature = "cli", arg(long))]
    pub base: u8,
    /// TMD title revision, such as 65535.
    #[cfg_attr(feature = "cli", arg(long))]
    pub revision: u16,
    /// Permit replacement of the verified Nintendo stub in slot 249 or 250.
    #[serde(default)]
    #[cfg_attr(feature = "cli", arg(long))]
    pub replace_stock_stub: bool,
}

impl CiosTarget {
    pub const fn title_id(&self) -> TitleId {
        TitleId::known(0x0000_0001_0000_0000 | self.slot as u64)
    }
}

pub type Cios = Wad<CiosOptions>;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(rename = "Cios")]
pub struct CiosOptions {
    pub target: CiosTarget,
}
