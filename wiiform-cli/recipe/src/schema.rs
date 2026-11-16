// SPDX-License-Identifier: GPL-2.0-only
use schemars::schema_for;

use crate::model::RecipeFile;

/// Generate a JSON Schema document for the user-authored YAML recipe shape.
///
/// Runtime checks still enforce menu-dependent removal allowlists and
/// filesystem constraints that static JSON Schema cannot express.
pub fn json_schema() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&schema_for!(RecipeFile))
}
