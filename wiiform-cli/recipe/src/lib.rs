// SPDX-License-Identifier: GPL-2.0-only
//! Declarative recipe types, validation, and editor schema.

mod country;
mod files;
mod hbc;
#[cfg_attr(test, allow(dead_code))] // Tests never download.
mod https;
mod priiloader;
mod recipe;
mod system;
mod titles;
mod wad;
pub mod yaml;

pub use country::CountryCode;
pub use files::{ResolvedPath, open_regular_file, read_bounded};
pub use hbc::Hbc;
pub use priiloader::{Autoboot, Hack, MountPoint, Priiloader, ReturnTo, ShutdownTo};
pub use recipe::Recipe;
pub use system::{Aspect, Display, Language, Menu, Nickname, Region, SensorBar, Setup};
pub use titles::{Cios, CiosBase, CiosSlot, CiosTarget, RemovableChannel, Title, TitleId};
pub use wad::{Sha256, Wad};

/// The JSON Schema of a recipe, for editors.
///
/// Menu-dependent rules and file checks are beyond what a schema can express;
/// reading a [`Recipe`] enforces those.
pub fn json_schema() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&schemars::schema_for!(Recipe))
}
