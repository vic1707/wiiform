// SPDX-License-Identifier: GPL-2.0-only
//! Strict Wiiform recipe parsing, normalization, and schema generation.
//!
//! The input model is deliberately separate from [`Recipe`]: input paths are
//! recipe-relative and removal aliases are expanded only after applying a
//! possible menu override. The workflow receives a validated, resolved model.

mod parser;
mod schema;
mod types;

pub use parser::{MAX_BYTES, MAX_DEPTH, MAX_EVENTS, RecipeError, load_recipe, parse_recipe};
pub use schema::json_schema;
pub use types::{
    Aspect, Autoboot, Display, HackSettings, Language, Menu, MountPoint, Priiloader,
    PriiloaderFile, PriiloaderSettings, Recipe, RecipeFile, RemovalInput, RemovalName, ReturnTo,
    Setup, Sha256, ShutdownTo, SignaturePolicy, Title, TitleFile, TitleId,
};
