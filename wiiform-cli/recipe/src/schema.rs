// SPDX-License-Identifier: GPL-2.0-only
use std::{borrow::Cow, marker::PhantomData};

use schemars::{JsonSchema, Schema, SchemaGenerator, schema_for};
use serde::{Deserialize, Deserializer};

use crate::Recipe;

/// Generate a JSON Schema document for the user-authored YAML recipe shape.
///
/// Runtime checks still enforce menu-dependent removal allowlists and
/// filesystem constraints that static JSON Schema cannot express.
pub fn json_schema() -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&schema_for!(Recipe))
}

/// Schemars helper for optional Serde fields where YAML null is forbidden.
pub(crate) struct NonNull<T>(PhantomData<T>);

impl<T: JsonSchema> JsonSchema for NonNull<T> {
    fn schema_name() -> Cow<'static, str> {
        T::schema_name()
    }

    fn schema_id() -> Cow<'static, str> {
        T::schema_id()
    }

    fn inline_schema() -> bool {
        T::inline_schema()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        T::json_schema(generator)
    }
}

/// Missing fields use Serde's default. A present field must contain a value.
pub(crate) fn non_null_option<'de, T: Deserialize<'de>, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}
