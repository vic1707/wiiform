// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::Deserialize;

use super::{TitleId, Wad};

/// A number that is only valid as one of a few values, checked where it is parsed.
macro_rules! restricted_u8 {
    ($(#[doc = $doc:literal])+ $name:ident, $what:literal, [$($value:literal),+ $(,)?]) => {
        $(#[doc = $doc])+
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Deserialize, JsonSchema)]
        #[serde(try_from = "u8")]
        #[schemars(!try_from, transparent)]
        pub struct $name(#[schemars(extend("enum" = [$($value),+]))] u8);

        impl $name {
            pub const fn get(self) -> u8 {
                self.0
            }
        }

        impl TryFrom<u8> for $name {
            type Error = anyhow::Error;

            fn try_from(value: u8) -> anyhow::Result<Self> {
                anyhow::ensure!(
                    [$($value),+].contains(&value),
                    concat!($what, " must be one of ", stringify!($($value),+))
                );
                Ok(Self(value))
            }
        }

        impl std::str::FromStr for $name {
            type Err = anyhow::Error;

            fn from_str(value: &str) -> anyhow::Result<Self> {
                value.parse::<u8>()?.try_into()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(formatter)
            }
        }
    };
}

restricted_u8! {
    /// IOS slot a cIOS may be installed to.
    CiosSlot, "cIOS slot", [248, 249, 250, 251]
}

restricted_u8! {
    /// Stock IOS a d2x cIOS is built on.
    CiosBase, "cIOS base", [38, 56, 57, 58]
}

/// Expected identity and replacement policy for a prebuilt Wii d2x WAD.
#[derive(Debug, Deserialize, JsonSchema)]
#[cfg_attr(feature = "cli", derive(clap::Args))]
#[serde(deny_unknown_fields)]
pub struct CiosTarget {
    /// Destination IOS slot (248–251).
    #[cfg_attr(feature = "cli", arg(long))]
    pub slot: CiosSlot,
    /// Base IOS declared by the d2x payload.
    #[cfg_attr(feature = "cli", arg(long))]
    pub base: CiosBase,
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
        TitleId::known(0x0000_0001_0000_0000 | self.slot.get() as u64)
    }
}

pub type Cios = Wad<CiosOptions>;

#[derive(Debug, Deserialize, JsonSchema)]
#[schemars(rename = "Cios")]
pub struct CiosOptions {
    pub target: CiosTarget,
}
