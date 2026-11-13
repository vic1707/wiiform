// SPDX-License-Identifier: GPL-2.0-only
use std::{borrow::Cow, fmt, str::FromStr};

use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, Serializer, de};

// Keep the Wii byte values, supported countries, and editor schema in one table.
macro_rules! country_codes {
    ($($name:ident = $code:literal => $alpha2:literal),+ $(,)?) => {
        /// Supported ISO alpha-2 country code and its Wii SYSCONF byte.
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        #[repr(u8)]
        pub enum CountryCode {
            $($name = $code),+
        }

        impl CountryCode {
            const ALL: &[Self] = &[$(Self::$name),+];

            pub const fn as_alpha2(self) -> &'static str {
                match self {
                    $(Self::$name => $alpha2),+
                }
            }

            fn from_alpha2(value: &str) -> Option<Self> {
                match value {
                    $($alpha2 => Some(Self::$name),)+
                    _ => None,
                }
            }
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
    Japan = 1 => "JP",
    Anguilla = 8 => "AI",
    AntiguaAndBarbuda = 9 => "AG",
    Argentina = 10 => "AR",
    Aruba = 11 => "AW",
    Bahamas = 12 => "BS",
    Barbados = 13 => "BB",
    Belize = 14 => "BZ",
    Bolivia = 15 => "BO",
    Brazil = 16 => "BR",
    BritishVirginIslands = 17 => "VG",
    Canada = 18 => "CA",
    CaymanIslands = 19 => "KY",
    Chile = 20 => "CL",
    Colombia = 21 => "CO",
    CostaRica = 22 => "CR",
    Dominica = 23 => "DM",
    DominicanRepublic = 24 => "DO",
    Ecuador = 25 => "EC",
    ElSalvador = 26 => "SV",
    FrenchGuiana = 27 => "GF",
    Grenada = 28 => "GD",
    Guadeloupe = 29 => "GP",
    Guatemala = 30 => "GT",
    Guyana = 31 => "GY",
    Haiti = 32 => "HT",
    Honduras = 33 => "HN",
    Jamaica = 34 => "JM",
    Martinique = 35 => "MQ",
    Mexico = 36 => "MX",
    Montserrat = 37 => "MS",
    // The Wii retains the historical code for the Netherlands Antilles.
    NetherlandsAntilles = 38 => "AN",
    Nicaragua = 39 => "NI",
    Panama = 40 => "PA",
    Paraguay = 41 => "PY",
    Peru = 42 => "PE",
    SaintKittsAndNevis = 43 => "KN",
    SaintLucia = 44 => "LC",
    SaintVincentAndTheGrenadines = 45 => "VC",
    Suriname = 46 => "SR",
    TrinidadAndTobago = 47 => "TT",
    TurksAndCaicosIslands = 48 => "TC",
    UnitedStates = 49 => "US",
    Uruguay = 50 => "UY",
    UsVirginIslands = 51 => "VI",
    Venezuela = 52 => "VE",
    Australia = 65 => "AU",
    Austria = 66 => "AT",
    Belgium = 67 => "BE",
    Denmark = 74 => "DK",
    Finland = 76 => "FI",
    France = 77 => "FR",
    Germany = 78 => "DE",
    Greece = 79 => "GR",
    Ireland = 82 => "IE",
    Italy = 83 => "IT",
    Luxembourg = 88 => "LU",
    Netherlands = 94 => "NL",
    NewZealand = 95 => "NZ",
    Norway = 96 => "NO",
    Poland = 97 => "PL",
    Portugal = 98 => "PT",
    Romania = 99 => "RO",
    Spain = 105 => "ES",
    Sweden = 107 => "SE",
    Switzerland = 108 => "CH",
    UnitedKingdom = 110 => "GB",
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
        if let Some(country) = Self::from_alpha2(value) {
            return Ok(country);
        }
        // Older recipes used the Wii's byte as a quoted decimal string.
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
        f.write_str(
            "unknown country code, expected an uppercase ISO alpha-2 code supported by the Wii",
        )
    }
}

impl std::error::Error for InvalidCountryCode {}

impl Serialize for CountryCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_alpha2())
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
            "anyOf": [
                {
                    "enum": Self::ALL.iter().map(|code| code.as_alpha2()).collect::<Vec<_>>()
                },
                {
                    "description": "Legacy quoted Wii country byte. Prefer the two-letter code.",
                    "enum": Self::ALL.iter().map(|code| code.as_u8().to_string()).collect::<Vec<_>>(),
                    "deprecated": true
                }
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_and_legacy_codes_map_to_the_same_wii_byte() {
        for &country in CountryCode::ALL {
            let alpha2 = country.as_alpha2();
            let legacy = country.as_u8().to_string();
            assert_eq!(alpha2.parse(), Ok(country));
            assert_eq!(legacy.parse(), Ok(country));
            assert_eq!(serde_json::to_value(country).unwrap(), alpha2);
        }
        assert_eq!(CountryCode::UnitedKingdom.as_alpha2(), "GB");
        assert_eq!(CountryCode::UnitedKingdom.as_u8(), 110);
        assert_eq!(CountryCode::UnitedStates.as_u8(), 49);
        assert_eq!(CountryCode::Japan.as_u8(), 1);
        assert_eq!(CountryCode::NetherlandsAntilles.as_alpha2(), "AN");
        assert!("ZZ".parse::<CountryCode>().is_err());
    }
}
