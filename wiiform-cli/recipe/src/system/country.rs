// SPDX-License-Identifier: GPL-2.0-only
use std::borrow::Cow;

use nnn::nnn;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[nnn(
    attrs(doc = "Quoted Wii country code accepted by a supported System Menu region."),
    derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize),
    nnn_derive(TryFrom, AsRef),
    validators(custom(with = valid_country_code, error = InvalidCountryCode))
)]
pub struct CountryCode(String);

fn valid_country_code(value: &str) -> bool {
    value.parse::<u8>().is_ok_and(|code| {
        matches!(code, 1 | 8..=52 | 65..=67 | 74 | 76..=79 | 82..=83 | 88 | 94..=99 | 105 | 107..=108 | 110)
    })
}

impl CountryCode {
    pub fn japan() -> Self {
        Self::try_from("1".to_owned()).expect("built-in Wii country code is valid")
    }

    pub fn united_states() -> Self {
        Self::try_from("49".to_owned()).expect("built-in Wii country code is valid")
    }

    pub fn united_kingdom() -> Self {
        Self::try_from("110".to_owned()).expect("built-in Wii country code is valid")
    }

    pub fn as_u8(&self) -> u8 {
        self.as_ref()
            .as_str()
            .parse()
            .expect("validated Wii country code is numeric")
    }

    pub fn is_americas(&self) -> bool {
        matches!(self.as_u8(), 8..=52)
    }

    pub fn is_pal(&self) -> bool {
        matches!(self.as_u8(), 65..=67 | 74 | 76..=79 | 82..=83 | 88 | 94..=99 | 105 | 107..=108 | 110)
    }
}

impl JsonSchema for CountryCode {
    fn schema_name() -> Cow<'static, str> {
        "CountryCode".into()
    }

    fn schema_id() -> Cow<'static, str> {
        concat!(module_path!(), "::CountryCode").into()
    }

    fn inline_schema() -> bool {
        true
    }

    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^(1|8|9|10|11|12|13|14|15|16|17|18|19|20|21|22|23|24|25|26|27|28|29|30|31|32|33|34|35|36|37|38|39|40|41|42|43|44|45|46|47|48|49|50|51|52|65|66|67|74|76|77|78|79|82|83|88|94|95|96|97|98|99|105|107|108|110)$",
            "description": "Quoted Wii country code compatible with the selected menu region. Common values are 1 for Japan, 49 for the United States, and 110 for the United Kingdom."
        })
    }
}
