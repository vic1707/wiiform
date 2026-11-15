// SPDX-License-Identifier: GPL-2.0-only
use nnn::nnn;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[nnn(
    attrs(
        doc = "Console nickname with 1–10 BMP letters, numbers, or spaces, including a letter or number.",
        schemars(extend("minLength" = 1, "maxLength" = 10))
    ),
    derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema),
    nnn_derive(TryFrom, AsRef<str>),
    default = "Wii".to_owned(),
    validators(
        custom(with = |name: &str| !name.trim().is_empty(), error = Nonblank),
        custom(with = |name: &str| name.chars().nth(10).is_none(), error = AtMostTenCharacters),
        custom(
            with = |name: &str| name.chars().all(|ch| {
                ch.len_utf16() == 1 && (ch.is_alphanumeric() || ch == ' ')
            }),
            error = InvalidChar
        )
    )
)]
pub struct Nickname(String);
