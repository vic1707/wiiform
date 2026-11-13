// SPDX-License-Identifier: GPL-2.0-only
use std::{borrow::Cow, fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

// Keep the Wii byte values, supported countries, and editor schema in one table.
macro_rules! country_codes {
    ($($name:ident = $code:literal),+ $(,)?) => {
        /// Nintendo Wii `IPL.SADR` country code.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u8)]
        pub enum CountryCode {
            $($name = $code),+
        }

        impl CountryCode {
            const ALL: &[Self] = &[$(Self::$name),+];
        }

        impl TryFrom<u8> for CountryCode {
            type Error = InvalidCountryCode;

            fn try_from(value: u8) -> Result<Self, Self::Error> {
                match value {
                    $($code => Ok(Self::$name),)+
                    _ => Err(InvalidCountryCode),
                }
            }
        }
    };
}

country_codes! {
    Japan = 1,
    Anguilla = 8,
    AntiguaAndBarbuda = 9,
    Argentina = 10,
    Aruba = 11,
    Bahamas = 12,
    Barbados = 13,
    Belize = 14,
    Bolivia = 15,
    Brazil = 16,
    BritishVirginIslands = 17,
    Canada = 18,
    CaymanIslands = 19,
    Chile = 20,
    Colombia = 21,
    CostaRica = 22,
    Dominica = 23,
    DominicanRepublic = 24,
    Ecuador = 25,
    ElSalvador = 26,
    FrenchGuiana = 27,
    Grenada = 28,
    Guadeloupe = 29,
    Guatemala = 30,
    Guyana = 31,
    Haiti = 32,
    Honduras = 33,
    Jamaica = 34,
    Martinique = 35,
    Mexico = 36,
    Montserrat = 37,
    NetherlandsAntilles = 38,
    Nicaragua = 39,
    Panama = 40,
    Paraguay = 41,
    Peru = 42,
    SaintKittsAndNevis = 43,
    SaintLucia = 44,
    SaintVincentAndTheGrenadines = 45,
    Suriname = 46,
    TrinidadAndTobago = 47,
    TurksAndCaicosIslands = 48,
    UnitedStates = 49,
    Uruguay = 50,
    UsVirginIslands = 51,
    Venezuela = 52,
    Australia = 65,
    Austria = 66,
    Belgium = 67,
    Denmark = 74,
    Finland = 76,
    France = 77,
    Germany = 78,
    Greece = 79,
    Ireland = 82,
    Italy = 83,
    Luxembourg = 88,
    Netherlands = 94,
    NewZealand = 95,
    Norway = 96,
    Poland = 97,
    Portugal = 98,
    Romania = 99,
    Spain = 105,
    Sweden = 107,
    Switzerland = 108,
    UnitedKingdom = 110,
}

impl CountryCode {
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub const fn is_americas(self) -> bool {
        matches!(self.as_u8(), 8..=52)
    }

    pub const fn is_pal(self) -> bool {
        self.as_u8() >= 65
    }
}

impl FromStr for CountryCode {
    type Err = InvalidCountryCode;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let code = value.parse::<u8>().map_err(|_| InvalidCountryCode)?;
        if code.to_string() != value {
            return Err(InvalidCountryCode);
        }
        Self::try_from(code)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InvalidCountryCode;

impl fmt::Display for InvalidCountryCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("unknown Wii country code")
    }
}

impl std::error::Error for InvalidCountryCode {}

impl Serialize for CountryCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.as_u8().to_string())
    }
}

impl<'de> Deserialize<'de> for CountryCode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(de::Error::custom)
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
            "enum": Self::ALL.iter().map(|code| code.as_u8().to_string()).collect::<Vec<_>>(),
            "description": "Quoted Nintendo Wii IPL.SADR country code. Codes map to named CountryCode variants."
        })
    }
}
