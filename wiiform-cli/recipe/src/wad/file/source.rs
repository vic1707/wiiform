// SPDX-License-Identifier: GPL-2.0-only
use std::path::PathBuf;

use base64::{Engine, engine::general_purpose::STANDARD};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer};

/// Where a recipe gets a file from.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Source {
    /// Local path, relative to the recipe.
    Path(PathBuf),
    /// HTTPS URL of the raw file.
    Url(String),
    /// The file itself, base64-encoded.
    Inline(
        #[serde(deserialize_with = "base64")]
        #[schemars(with = "String", extend("contentEncoding" = "base64"))]
        Vec<u8>,
    ),
}

fn base64<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
    let text = String::deserialize(deserializer)?;
    let compact: String = text.split_ascii_whitespace().collect();
    STANDARD.decode(compact).map_err(serde::de::Error::custom)
}
