// SPDX-License-Identifier: GPL-2.0-only
use derive_more::Display;
use nnn::nnn;
use schemars::JsonSchema;
use serde::Deserialize;

#[nnn(
    attrs(
        doc = "RVLoader release number: major.minor, without the leading v.",
        schemars(rename = "RvloaderVersion", extend("pattern" = "^[0-9]+\\.[0-9]+$"))
    ),
    derive(Clone, Debug, Display, Eq, PartialEq, Deserialize, JsonSchema),
    nnn_derive(TryFrom, FromStr),
    validators(predicate(
        with = |version: &str| {
            let number = |part: &str| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
            version.split_once('.').is_some_and(|(major, minor)| number(major) && number(minor))
        },
        error_name = MajorMinor
    ))
)]
pub struct Version(String);
