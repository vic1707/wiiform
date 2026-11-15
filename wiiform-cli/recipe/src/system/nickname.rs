// SPDX-License-Identifier: GPL-2.0-only
use nnn::nnn;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use unicode_general_category::{GeneralCategory, get_general_category};

#[nnn(
    attrs(
        doc = "Nonblank console nickname with 1–10 printable BMP characters.",
        schemars(extend("minLength" = 1, "maxLength" = 10))
    ),
    derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema),
    nnn_derive(TryFrom, AsRef<str>, FromStr),
    default = "Wii".to_owned(),
    validators(
        custom(with = |name: &str| name.chars().nth(10).is_none(), error = AtMostTenCharacters),
        custom(with = |name: &str| !name.trim().is_empty(), error = Nonblank),
        custom(
            with = |name: &str| name.chars().all(|c| {
                c.len_utf16() == 1
                    && !matches!(
                        get_general_category(c),
                        GeneralCategory::Control
                            | GeneralCategory::Format
                            | GeneralCategory::PrivateUse
                            | GeneralCategory::Surrogate
                            | GeneralCategory::Unassigned
                    )
            }),
            error = PrintableBmpCharacters
        )
    )
)]
pub struct Nickname(String);

#[cfg(test)]
mod tests {
    use super::{Nickname, NicknameError, NicknameParseError};

    #[test]
    fn validates_wii_characters_and_counts_them_instead_of_utf8_bytes() {
        assert!("éééééééééé".parse::<Nickname>().is_ok());
        for (input, expected) in [
            ("ééééééééééé", NicknameError::AtMostTenCharacters),
            ("  ", NicknameError::Nonblank),
            ("Wii🎮", NicknameError::PrintableBmpCharacters),
            ("Wii\u{200d}", NicknameError::PrintableBmpCharacters),
            ("Wii\u{e000}", NicknameError::PrintableBmpCharacters),
        ] {
            assert_eq!(
                input.parse::<Nickname>(),
                Err(NicknameParseError::Validation(expected))
            );
        }
    }
}
