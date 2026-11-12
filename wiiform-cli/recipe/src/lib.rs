// SPDX-License-Identifier: GPL-2.0-only
//! Declarative recipe types, validation, and editor schema.

mod model;
mod parser;
pub mod priiloader;
mod schema;
pub mod system;
pub mod titles;

pub use model::{Recipe, RecipeFile};
pub use parser::{
    MAX_BYTES, MAX_DEPTH, MAX_EVENTS, RecipeError, load_recipe, load_recipe_bytes, parse_recipe,
};
pub use priiloader::{
    Autoboot, HackSettings, MountPoint, Priiloader, PriiloaderFile, PriiloaderSettings, ReturnTo,
    ShutdownTo,
};
pub use schema::json_schema;
pub use system::{Aspect, Display, Language, Menu, Nickname, Setup};
pub use titles::{RemovalInput, RemovalName, Sha256, SignaturePolicy, Title, TitleFile, TitleId};
