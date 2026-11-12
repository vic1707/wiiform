// SPDX-License-Identifier: GPL-2.0-only
use std::borrow::Cow;

use nnn::nnn;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[nnn(
    derive(
        Debug,
        Clone,
        PartialEq,
        Eq,
        PartialOrd,
        Ord,
        Hash,
        Serialize,
        Deserialize
    ),
    nnn_derive(TryFrom, AsRef, Deref),
    sanitizers(lowercase),
    validators(regex = r"^[0-9a-f]{64}$")
)]
pub struct Sha256(String);

impl std::str::FromStr for Sha256 {
    type Err = Sha256Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl JsonSchema for Sha256 {
    fn schema_name() -> Cow<'static, str> {
        "Sha256".into()
    }
    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::Sha256").into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 64,
            "maxLength": 64,
            "pattern": "^[0-9a-fA-F]{64}$",
            "description": "64-character hexadecimal SHA-256 digest; normalized to lowercase"
        })
    }
}

impl Sha256 {
    pub fn as_str(&self) -> &str {
        self.as_ref().as_str()
    }
}
