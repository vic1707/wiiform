// SPDX-License-Identifier: GPL-2.0-only
//! Title identifiers, WAD requests, and channel removals.
use std::{num::NonZeroU64, str::FromStr};

use anyhow::{Context, Result};
use hex::FromHex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::{Data, Menu};

/// Nonzero Wii title ID, written as 16 hex digits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TitleId(NonZeroU64);

impl TitleId {
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(id) => Some(Self(id)),
            None => None,
        }
    }

    const fn known(value: u64) -> Self {
        Self::new(value).expect("built-in title IDs are nonzero")
    }

    /// The upper half, which names the kind of title (system, channel, ...).
    pub const fn kind(self) -> u32 {
        (self.0.get() >> 32) as u32
    }

    pub const fn as_u64(self) -> u64 {
        self.0.get()
    }
}

impl FromStr for TitleId {
    type Err = anyhow::Error;

    fn from_str(value: &str) -> Result<Self> {
        let bytes =
            <[u8; 8]>::from_hex(value).context("expected a 16-digit hexadecimal title ID")?;
        Self::new(u64::from_be_bytes(bytes)).context("title ID must be nonzero")
    }
}

impl std::fmt::Display for TitleId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:016x}", self.0)
    }
}

/// SHA-256 of the exact binary payload, encoded as 64 hex digits.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(transparent)]
pub struct Sha256(
    #[serde(with = "hex")]
    #[schemars(
        with = "String",
        length(equal = 64),
        regex(pattern = "^[0-9a-fA-F]{64}$")
    )]
    pub [u8; 32],
);

impl FromStr for Sha256 {
    type Err = hex::FromHexError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        <[u8; 32]>::from_hex(value).map(Self)
    }
}

/// A WAD to install when its title is absent, or to verify when it is present.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Title {
    pub source: Data,
    /// SHA-256 of the exact WAD bytes.
    pub sha256: Sha256,
    /// Permit an unsigned community channel after content validation.
    /// By default the WAD must carry Nintendo signatures.
    #[serde(default)]
    pub allow_unsigned: bool,
}

/// A prebuilt Wii d2x cIOS WAD and the slot it is expected to fill.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Cios {
    pub source: Data,
    /// SHA-256 of the exact WAD bytes.
    pub sha256: Sha256,
    pub target: CiosTarget,
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
        TitleId::known(0x0000_0001_0000_0000 | self.slot.0 as u64)
    }
}

/// A number that is only valid as one of a few values, checked where it is parsed.
macro_rules! restricted_u8 {
    ($(#[doc = $doc:literal] $name:ident, $what:literal, [$($value:literal),+];)+) => {$(
        #[doc = $doc]
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Deserialize, JsonSchema)]
        #[serde(try_from = "u8")]
        #[schemars(!try_from, transparent)]
        pub struct $name(#[schemars(extend("enum" = [$($value),+]))] u8);

        impl From<$name> for u8 {
            fn from(value: $name) -> Self {
                value.0
            }
        }

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

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(formatter)
            }
        }
    )+};
}

restricted_u8! {
    /// IOS slot a cIOS may be installed to.
    CiosSlot, "cIOS slot", [248, 249, 250, 251];
    /// Stock IOS a d2x cIOS is built on.
    CiosBase, "cIOS base", [38, 56, 57, 58];
}

/// A stock channel that `remove` can take off the image.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RemovableChannel {
    /// The menu region's Forecast Channel and its common stub.
    ForecastChannel,
    /// The menu region's News Channel and its common stub.
    NewsChannel,
    /// Wii Shop Channel (HABA).
    WiiShopChannel,
    /// Photo Channel versions HAAA and HAYA.
    PhotoChannel,
}

impl RemovableChannel {
    /// The titles that make up this channel on a console of the menu's region.
    pub fn title_ids(self, menu: Menu) -> Vec<TitleId> {
        let regional = |base: u64| {
            [menu.channel_suffix(), b'A']
                .map(|suffix| TitleId::known(base | u64::from(suffix)))
                .to_vec()
        };
        match self {
            Self::ForecastChannel => regional(0x0001_0002_4841_4600),
            Self::NewsChannel => regional(0x0001_0002_4841_4700),
            Self::WiiShopChannel => vec![TitleId::known(0x0001_0002_4841_4241)],
            Self::PhotoChannel => vec![
                TitleId::known(0x0001_0002_4841_4141),
                TitleId::known(0x0001_0002_4841_5941),
            ],
        }
    }
}
