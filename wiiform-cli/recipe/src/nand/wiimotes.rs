// SPDX-License-Identifier: GPL-2.0-only
//! The `wiimotes` key: the Wiimotes the console has registered, kept from the seed.
//! They are written into SYSCONF with the system settings, so `setup` is required.
use schemars::JsonSchema;
use serde::Deserialize;

/// The registered Wiimotes, copied from the seed's console.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Wiimotes {
    /// Copy the registrations the seed holds.
    Keep,
}
