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
    validators(
        regex = r"^[0-9a-f]{16}$",
        custom(with = |value: &String| value != "0000000000000000", error = Zero)
    )
)]
pub struct TitleId(String);

impl std::str::FromStr for TitleId {
    type Err = TitleIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl JsonSchema for TitleId {
    fn schema_name() -> Cow<'static, str> {
        "TitleId".into()
    }
    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::TitleId").into()
    }
    fn inline_schema() -> bool {
        true
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 16,
            "maxLength": 16,
            "pattern": "^[0-9a-fA-F]{16}$",
            "not": {"const": "0000000000000000"},
            "description": "Nonzero 16-character hexadecimal Wii title ID, normalized to lowercase"
        })
    }
}

impl TitleId {
    pub fn as_str(&self) -> &str {
        self.as_ref().as_str()
    }

    pub fn as_u64(&self) -> u64 {
        u64::from_str_radix(self.as_str(), 16).expect("validated title ID is hexadecimal")
    }
}
