// SPDX-License-Identifier: GPL-2.0-only
//! Declarative recipe types, validation, and editor schema.

mod parser;
mod schema;
mod types;

pub use parser::{
    MAX_BYTES, MAX_DEPTH, MAX_EVENTS, RecipeError, load_recipe, load_recipe_bytes, parse_recipe,
};
pub use schema::json_schema;
pub use types::{
    Aspect, Autoboot, Display, HackSettings, Language, Menu, MountPoint, Priiloader,
    PriiloaderFile, PriiloaderSettings, Recipe, RecipeFile, RemovalInput, RemovalName, ReturnTo,
    Setup, Sha256, ShutdownTo, SignaturePolicy, Title, TitleFile, TitleId,
};
