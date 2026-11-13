// SPDX-License-Identifier: GPL-2.0-only
use std::{borrow::Cow, sync::LazyLock};

use nnn::nnn;
use regex::Regex;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[nnn(
    attrs(doc = "Nonblank console nickname with 1–10 printable BMP characters."),
    derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize),
    nnn_derive(TryFrom, AsRef),
    validators(custom(with = valid_nickname, error = InvalidNickname))
)]
pub struct Nickname(String);

fn valid_nickname(value: &str) -> bool {
    // Match the native printable-character rule by excluding Unicode Other
    // categories. The BMP restriction also keeps each character to one UTF-16 unit.
    static PRINTABLE_BMP: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\A[\x{0000}-\x{FFFF}&&\P{C}]{1,10}\z").expect("the nickname pattern is valid")
    });
    !value.trim().is_empty() && PRINTABLE_BMP.is_match(value)
}

impl Nickname {
    pub fn as_str(&self) -> &str {
        self.as_ref().as_str()
    }
}

impl Default for Nickname {
    fn default() -> Self {
        Self::try_from("Wii".to_owned()).expect("built-in Wii nickname is valid")
    }
}

impl std::str::FromStr for Nickname {
    type Err = NicknameError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from(value.to_owned())
    }
}

impl JsonSchema for Nickname {
    fn schema_name() -> Cow<'static, str> {
        "Nickname".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::Nickname").into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "minLength": 1,
            "maxLength": 10,
            "description": "Nonblank console nickname with 1–10 printable BMP characters"
        })
    }
}
