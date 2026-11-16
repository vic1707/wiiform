// SPDX-License-Identifier: GPL-2.0-only
use std::{num::NonZeroU64, str::FromStr};

use anyhow::{Context, Result};
use derive_more::Display;
use hex::FromHex;

/// Nonzero Wii title ID, written as 16 hex digits.
#[derive(Clone, Copy, Debug, Display, Eq, PartialEq)]
#[display("{_0:016x}")]
pub struct TitleId(NonZeroU64);

impl TitleId {
    pub const fn new(value: u64) -> Option<Self> {
        match NonZeroU64::new(value) {
            Some(id) => Some(Self(id)),
            None => None,
        }
    }

    pub(crate) const fn known(value: u64) -> Self {
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
