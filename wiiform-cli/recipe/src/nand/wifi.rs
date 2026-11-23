// SPDX-License-Identifier: GPL-2.0-only
//! The `wifi` key: the console's Wi-Fi profile, kept from the seed or set here.
//! The profile is `/shared2/sys/net/02/config.dat`, and only its first profile
//! is written.
use std::{fmt, path::PathBuf};

use anyhow::{Result, bail, ensure};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize, de::Error as _};

use crate::source::Source;

/// Length of the Wi-Fi profile file, one 0x4000-byte cluster's worth at most.
pub const CONFIG_BYTES: usize = 7004;
/// Network type byte: wireless.
const WIRELESS: u8 = 1;
/// Selected-profile byte: profile 1 is in use.
const PROFILE_SELECTED: u8 = 0xa6;
const NETWORK_TYPE_AT: usize = 4;
const SELECTED_AT: usize = 8;
/// Profile 1 starts with its network name, 32 bytes at most, and that name's length.
const SSID_AT: usize = 1996;
const SSID_LENGTH_AT: usize = 2029;
const SSID_MAX: usize = 32;
const SECURITY_AT: usize = 2033;
/// The passphrase's length, then its bytes, 63 at most.
const PASSPHRASE_LENGTH_AT: usize = 2037;
const PASSPHRASE_AT: usize = 2040;
const PASSPHRASE_MIN: usize = 8;
const PASSPHRASE_MAX: usize = 63;

/// The Wi-Fi profile: `keep` copies the seed's, `set` writes the first profile.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Wifi {
    /// Copy the Wi-Fi profile the seed holds.
    Keep,
    /// Write the first profile with these settings.
    Set(Profile),
}

impl Wifi {
    /// Checks what the type cannot express.
    pub fn check(&self) -> Result<()> {
        match self {
            Self::Keep => Ok(()),
            Self::Set(profile) => profile.check(),
        }
    }
}

/// The first Wi-Fi profile, as the console's network settings show it.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    /// Network name, 1 to 32 bytes.
    pub ssid: String,
    /// Security of the network.
    pub security: Security,
    /// Required with wpa2_aes, and absent with open. Give `text`, or `file` with a
    /// path relative to the recipe; one final newline of the file is not part of it.
    #[serde(default)]
    #[schemars(with = "Option<PassphraseInput>")]
    pub passphrase: Option<Passphrase>,
}

impl Profile {
    /// Checks the name, and that a passphrase is given exactly when the security needs one.
    pub fn check(&self) -> Result<()> {
        let ssid = self.ssid.as_bytes();
        ensure!(
            (1..=SSID_MAX).contains(&ssid.len()) && !ssid.contains(&0),
            "wifi.set.ssid is 1 to 32 bytes with no NUL"
        );
        match (self.security, &self.passphrase) {
            (Security::Open, None) | (Security::Wpa2Aes, Some(_)) => Ok(()),
            (Security::Open, Some(_)) => {
                bail!("wifi.set.passphrase must be absent with security open")
            }
            (Security::Wpa2Aes, None) => {
                bail!("wifi.set.passphrase is required with security wpa2_aes")
            }
        }
    }

    /// The whole profile file the console reads.
    pub fn config(&self) -> Result<Vec<u8>> {
        self.check()?;
        encode_file(&Fields {
            ssid: self.ssid.as_bytes().to_vec(),
            security: self.security,
            passphrase: self
                .passphrase
                .as_ref()
                .map(|passphrase| passphrase.0.clone()),
        })
    }
}

/// The fields of the first profile, as the file holds them: bytes, since a file the
/// console wrote may hold anything the layout allows.
pub struct Fields {
    pub ssid: Vec<u8>,
    pub security: Security,
    pub passphrase: Option<Vec<u8>>,
}

/// The profile file for `fields`, with everything else zero. Only the sizes are
/// checked here; the recipe's own rules for a passphrase apply before this.
pub fn encode_file(fields: &Fields) -> Result<Vec<u8>> {
    let ssid = &fields.ssid;
    ensure!(
        ssid.len() <= SSID_MAX && !ssid.contains(&0),
        "a network name is at most 32 bytes with no NUL"
    );
    let passphrase = fields.passphrase.as_deref().unwrap_or_default();
    ensure!(
        passphrase.len() <= PASSPHRASE_MAX,
        "a passphrase is at most 63 bytes"
    );
    let mut file = vec![0u8; CONFIG_BYTES];
    file[NETWORK_TYPE_AT] = WIRELESS;
    file[SELECTED_AT] = PROFILE_SELECTED;
    file[SSID_LENGTH_AT] = ssid.len() as u8;
    file[SSID_AT..SSID_AT + ssid.len()].copy_from_slice(ssid);
    file[SECURITY_AT] = fields.security.code();
    file[PASSPHRASE_LENGTH_AT] = passphrase.len() as u8;
    file[PASSPHRASE_AT..PASSPHRASE_AT + passphrase.len()].copy_from_slice(passphrase);
    Ok(file)
}

/// The fields a profile file holds, when they have this layout's security and sizes.
/// The caller checks that `encode_file` gives back the same file.
pub fn decode_file(file: &[u8]) -> Option<Fields> {
    if file.len() != CONFIG_BYTES {
        return None;
    }
    let ssid_length = usize::from(file[SSID_LENGTH_AT]);
    let passphrase_length = usize::from(file[PASSPHRASE_LENGTH_AT]);
    if ssid_length > SSID_MAX || passphrase_length > PASSPHRASE_MAX {
        return None;
    }
    let security = match file[SECURITY_AT] {
        0x00 => Security::Open,
        0x05 => Security::Wpa2Aes,
        _ => return None,
    };
    Some(Fields {
        ssid: file[SSID_AT..SSID_AT + ssid_length].to_vec(),
        security,
        passphrase: (passphrase_length > 0)
            .then(|| file[PASSPHRASE_AT..PASSPHRASE_AT + passphrase_length].to_vec()),
    })
}

/// The security of a Wi-Fi network. Only these two are written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Security {
    /// No security.
    Open,
    /// WPA2-PSK with AES.
    Wpa2Aes,
}

impl Security {
    /// The profile's security byte.
    fn code(self) -> u8 {
        match self {
            Self::Open => 0x00,
            Self::Wpa2Aes => 0x05,
        }
    }
}

/// Where a passphrase comes from.
#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum PassphraseInput {
    /// The passphrase itself.
    Text(String),
    /// A file holding the passphrase, relative to the recipe.
    File(PathBuf),
}

/// A Wi-Fi passphrase: 8 to 63 printable ASCII characters. `Debug` hides it.
#[derive(Clone, PartialEq, Eq)]
pub struct Passphrase(Vec<u8>);

impl Passphrase {
    fn checked(bytes: Vec<u8>) -> Result<Self> {
        ensure!(
            (PASSPHRASE_MIN..=PASSPHRASE_MAX).contains(&bytes.len()),
            "a Wi-Fi passphrase is 8 to 63 characters"
        );
        ensure!(
            bytes.iter().all(|byte| (0x20..=0x7e).contains(byte)),
            "a Wi-Fi passphrase is printable ASCII only"
        );
        Ok(Self(bytes))
    }
}

impl fmt::Debug for Passphrase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Passphrase(<hidden>)")
    }
}

impl<'de> Deserialize<'de> for Passphrase {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let bytes = match PassphraseInput::deserialize(deserializer)? {
            PassphraseInput::Text(text) => text.into_bytes(),
            PassphraseInput::File(path) => {
                // The file may end in one newline, so it can be one byte longer.
                let file = Source::Path(path)
                    .read(1..=PASSPHRASE_MAX + 2)
                    .map_err(|error| D::Error::custom(format!("{error:#}")))?;
                without_final_newline(file.bytes)
            }
        };
        Passphrase::checked(bytes).map_err(|error| D::Error::custom(format!("{error:#}")))
    }
}

fn without_final_newline(mut bytes: Vec<u8>) -> Vec<u8> {
    if bytes.last() == Some(&b'\n') {
        bytes.pop();
        if bytes.last() == Some(&b'\r') {
            bytes.pop();
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use anyhow::Context;

    use super::*;

    fn profile(security: Security, passphrase: Option<&str>) -> Profile {
        Profile {
            ssid: "HomeNet".to_owned(),
            security,
            passphrase: passphrase.map(|text| Passphrase(text.as_bytes().to_vec())),
        }
    }

    #[test]
    fn the_profile_file_holds_the_fields_where_the_console_reads_them() -> Result<()> {
        let file = profile(Security::Wpa2Aes, Some("correct horse")).config()?;
        assert_eq!(file.len(), CONFIG_BYTES);
        assert_eq!(file[NETWORK_TYPE_AT], WIRELESS);
        assert_eq!(file[SELECTED_AT], PROFILE_SELECTED);
        assert_eq!(&file[SSID_AT..SSID_AT + 7], b"HomeNet");
        assert_eq!(file[SSID_LENGTH_AT], 7);
        assert_eq!(file[SECURITY_AT], 0x05);
        assert_eq!(file[PASSPHRASE_LENGTH_AT], 13);
        assert_eq!(&file[PASSPHRASE_AT..PASSPHRASE_AT + 13], b"correct horse");
        // Nothing else in the file is set: profile 2 and 3 stay zero.
        let set: Vec<usize> = (0..CONFIG_BYTES).filter(|&at| file[at] != 0).collect();
        let mut expected: Vec<usize> = [
            NETWORK_TYPE_AT,
            SELECTED_AT,
            SECURITY_AT,
            SSID_LENGTH_AT,
            PASSPHRASE_LENGTH_AT,
        ]
        .into_iter()
        .chain(SSID_AT..SSID_AT + 7)
        .chain(PASSPHRASE_AT..PASSPHRASE_AT + 13)
        .collect();
        expected.sort_unstable();
        assert_eq!(set, expected);
        Ok(())
    }

    #[test]
    fn open_networks_take_no_passphrase_and_wpa2_needs_one() {
        assert!(profile(Security::Open, None).config().is_ok());
        assert!(
            profile(Security::Open, Some("correct horse"))
                .config()
                .is_err()
        );
        assert!(profile(Security::Wpa2Aes, None).config().is_err());
    }

    #[test]
    fn passphrases_are_8_to_63_printable_ascii() {
        assert!(Passphrase::checked(b"1234567".to_vec()).is_err());
        assert!(Passphrase::checked(b"12345678".to_vec()).is_ok());
        assert!(Passphrase::checked(vec![b'a'; 63]).is_ok());
        assert!(Passphrase::checked(vec![b'a'; 64]).is_err());
        assert!(Passphrase::checked(b"pass\nword!".to_vec()).is_err());
    }

    #[test]
    fn a_passphrase_file_loses_only_its_final_newline() {
        assert_eq!(without_final_newline(b"abc\n".to_vec()), b"abc");
        assert_eq!(without_final_newline(b"abc\r\n".to_vec()), b"abc");
        assert_eq!(without_final_newline(b"abc\n\n".to_vec()), b"abc\n");
        assert_eq!(without_final_newline(b"abc".to_vec()), b"abc");
    }

    #[test]
    fn decoding_gives_back_the_fields_it_was_encoded_from() -> Result<()> {
        for security in [Security::Open, Security::Wpa2Aes] {
            let fields = Fields {
                ssid: b"HomeNet".to_vec(),
                security,
                passphrase: (security == Security::Wpa2Aes).then(|| b"correct horse".to_vec()),
            };
            let file = encode_file(&fields)?;
            let decoded = decode_file(&file).context("the layout describes its own file")?;
            assert_eq!(decoded.ssid, fields.ssid);
            assert_eq!(decoded.security, fields.security);
            assert_eq!(decoded.passphrase, fields.passphrase);
        }
        // Another security, or a file of another size, is not described.
        let mut wep = encode_file(&Fields {
            ssid: b"HomeNet".to_vec(),
            security: Security::Open,
            passphrase: None,
        })?;
        wep[SECURITY_AT] = 0x01;
        assert!(decode_file(&wep).is_none());
        assert!(decode_file(&wep[..10]).is_none());
        Ok(())
    }

    #[test]
    fn debug_output_hides_the_passphrase() {
        let text = format!("{:?}", profile(Security::Wpa2Aes, Some("correct horse")));
        assert!(!text.contains("correct horse"), "{text}");
    }
}
