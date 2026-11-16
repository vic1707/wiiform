// SPDX-License-Identifier: GPL-2.0-only
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// Keep the Wii byte values, supported countries, and editor schema in one table.
macro_rules! country_codes {
    ($($name:ident = $code:literal => $alpha2:literal),+ $(,)?) => {
        /// Supported ISO alpha-2 country code and its Wii SYSCONF byte.
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Deserialize, Serialize, JsonSchema)]
        #[repr(u8)]
        pub enum CountryCode {
            $(#[serde(rename = $alpha2)] $name = $code),+
        }

        impl CountryCode {
            #[cfg(test)]
            const ALL: &[Self] = &[$(Self::$name),+];

            pub const fn as_alpha2(self) -> &'static str {
                match self {
                    $(Self::$name => $alpha2),+
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
    /// The byte SYSCONF stores for this country.
    pub const fn as_u8(self) -> u8 {
        self as u8
    }

    pub(crate) const fn is_americas(self) -> bool {
        matches!(self.as_u8(), 8..=52)
    }

    pub(crate) const fn is_pal(self) -> bool {
        self.as_u8() >= 65
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_codes_map_to_their_wii_bytes() {
        for &country in CountryCode::ALL {
            let alpha2 = country.as_alpha2();
            assert_eq!(
                serde_saphyr::from_str::<CountryCode>(alpha2).unwrap(),
                country
            );
            assert_eq!(serde_json::to_value(country).unwrap(), alpha2);
        }
        assert_eq!(CountryCode::UnitedKingdom.as_alpha2(), "GB");
        assert_eq!(CountryCode::UnitedKingdom.as_u8(), 110);
        assert_eq!(CountryCode::UnitedStates.as_u8(), 49);
        assert_eq!(CountryCode::Japan.as_u8(), 1);
        assert_eq!(CountryCode::NetherlandsAntilles.as_alpha2(), "AN");
        assert!(serde_saphyr::from_str::<CountryCode>("ZZ").is_err());
        assert!(serde_saphyr::from_str::<CountryCode>("110").is_err());
    }
}
