// SPDX-License-Identifier: GPL-2.0-only
//! Declarative recipe types, validation, and editor schema.

mod country;
mod data;
mod files;
mod hbc;
mod priiloader;
mod recipe;
mod system;
mod titles;
pub mod yaml;

pub use country::CountryCode;
pub use data::Data;
pub use files::{ResolvedPath, open_regular_file, read_bounded};
pub use hbc::Hbc;
pub use priiloader::{Autoboot, Hack, MountPoint, Priiloader, ReturnTo, ShutdownTo};
pub use recipe::Recipe;
pub use system::{Aspect, Display, Language, Menu, Nickname, Region, SensorBar, Setup};
pub use titles::{Cios, CiosBase, CiosSlot, CiosTarget, RemovableChannel, Sha256, Title, TitleId};

/// The JSON Schema of a recipe, for editors.
///
/// Menu-dependent rules and file checks are beyond what a schema can express;
/// [`Recipe::load`] enforces those.
pub fn json_schema() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&schemars::schema_for!(Recipe))
}
