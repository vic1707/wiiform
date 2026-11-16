// SPDX-License-Identifier: GPL-2.0-only
//! Prebuilt d2x cIOS WADs: the `cios` key.
use std::str::FromStr;

use anyhow::Result;
use derive_more::{Display, Into};
use schemars::JsonSchema;
use serde::Deserialize;

use crate::{TitleId, Wad};

/// A prebuilt Wii d2x cIOS WAD and the slot it is expected to fill.
#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(deny_unknown_fields)]
pub struct Cios {
    #[serde(flatten)]
    pub wad: Wad,
    pub target: Target,
}

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
    pub const fn title_id(&self) -> TitleId {
        TitleId::known(0x0000_0001_0000_0000 | self.slot.0 as u64)
    }
}

/// A number that is only valid as one of a few values, checked where it is parsed.
macro_rules! restricted_u8 {
    ($(#[doc = $doc:literal] $name:ident, $what:literal, [$($value:literal),+];)+) => {$(
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Display, Into, Eq, PartialEq, Hash, Deserialize, JsonSchema)]
        #[serde(try_from = "u8")]
        #[schemars(!try_from, transparent)]
        pub struct $name(#[schemars(extend("enum" = [$($value),+]))] u8);

        impl TryFrom<u8> for $name {
            type Error = anyhow::Error;

            fn try_from(value: u8) -> Result<Self> {
                anyhow::ensure!(
                    [$($value),+].contains(&value),
                    concat!($what, " must be one of ", stringify!($($value),+))
                );
                Ok(Self(value))
            }
        }

        impl FromStr for $name {
            type Err = anyhow::Error;

            fn from_str(value: &str) -> Result<Self> {
                value.parse::<u8>()?.try_into()
            }
        }
    )+};
}

restricted_u8! {
    /// IOS slot a cIOS may be installed to.
    Slot, "cIOS slot", [248, 249, 250, 251];
    /// Stock IOS a d2x cIOS is built on.
    Base, "cIOS base", [38, 56, 57, 58];
}
