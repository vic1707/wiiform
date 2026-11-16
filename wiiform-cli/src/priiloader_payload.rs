// SPDX-License-Identifier: GPL-2.0-only
//! Priiloader 0.10.0, redistributed unmodified from its release (see `THIRD_PARTY.md`).
//! The library checks both payloads against their pinned hashes before writing them.

/// The NAND application Priiloader's own installer writes.
pub const APP: &[u8] = include_bytes!("../assets/priiloader/priiloader.app");
/// The System Menu hack definitions that ship with it.
pub const DEFINITIONS: &[u8] = include_bytes!("../assets/priiloader/hacks_hash.ini");
