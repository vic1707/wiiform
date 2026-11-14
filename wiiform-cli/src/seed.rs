// SPDX-License-Identifier: GPL-2.0-only
//! Validated, decoded inputs from one Wii seed.

use std::io::Read;

use anyhow::{Context, Result, anyhow, ensure};
use lzma_rust2::XzReader;
use nnn::nnn;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Sha256Hasher};
use wiiform_recipe::{
    Sha256,
    yaml::{self, Binary},
};

const BOOT_BYTES: usize = 0x108000;
const KEYS_BYTES: usize = 0x400;
const MAX_XZ_MEMORY: u64 = 128 * 1024 * 1024;

#[nnn(
    derive(Clone, Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize),
    nnn_derive(TryFrom, AsRef),
    validators(custom(with = valid_setting_value, error = InvalidConsoleSettingValue))
)]
pub struct ConsoleSettingValue(String);

fn valid_setting_value(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

impl ConsoleSettingValue {
    pub fn as_str(&self) -> &str {
        self.as_ref().as_str()
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ConsoleSettings {
    #[serde(rename = "AREA")]
    pub area: ConsoleSettingValue,
    #[serde(rename = "MODEL")]
    pub model: ConsoleSettingValue,
    #[serde(rename = "DVD")]
    pub dvd: ConsoleSettingValue,
    #[serde(rename = "MPCH")]
    pub mpch: ConsoleSettingValue,
    #[serde(rename = "CODE")]
    pub code: ConsoleSettingValue,
    #[serde(rename = "SERNO")]
    pub serial: ConsoleSettingValue,
    #[serde(rename = "VIDEO")]
    pub video: ConsoleSettingValue,
    #[serde(rename = "GAME")]
    pub game: ConsoleSettingValue,
}

/// Seed components checked before recipe overrides are applied.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockSeed {
    keys: Vec<u8>,
    boot_blocks: Vec<u8>,
    bad_blocks: Vec<u16>,
    settings: ConsoleSettings,
    settings_sha256: [u8; 32],
}

impl StockSeed {
    pub fn from_reader<R: Read>(reader: R) -> Result<Self> {
        let manifest: SeedManifest =
            serde_saphyr::from_reader_with_options(reader, yaml::options())
                .map_err(|_| anyhow!("Invalid Wii seed YAML"))?;

        let keys = manifest.keys.footer.0;
        ensure!(
            keys.len() == KEYS_BYTES,
            "Seed keys footer has the wrong size"
        );
        ensure!(
            sha256(&keys) == manifest.keys.sha256.as_str(),
            "Keys do not match their SHA-256"
        );

        let boot_blocks = decode_xz(&manifest.boot_blocks.xz.0)?;
        ensure!(
            sha256(&boot_blocks) == manifest.boot_blocks.sha256.as_str(),
            "Boot blocks do not match their SHA-256"
        );

        Ok(Self {
            keys,
            boot_blocks,
            bad_blocks: manifest.bad_blocks,
            settings: manifest.settings.fields,
            settings_sha256: manifest.settings.sha256.to_bytes(),
        })
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        Self::from_reader(bytes)
    }

    pub fn keys(&self) -> &[u8] {
        &self.keys
    }

    pub fn boot_blocks(&self) -> &[u8] {
        &self.boot_blocks
    }

    pub fn bad_blocks(&self) -> &[u16] {
        &self.bad_blocks
    }

    pub fn settings(&self) -> &ConsoleSettings {
        &self.settings
    }

    pub fn settings_sha256(&self) -> &[u8; 32] {
        &self.settings_sha256
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SeedManifest {
    keys: KeyFooter,
    bad_blocks: Vec<u16>,
    boot_blocks: BootBlocks,
    settings: Settings,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    info: Option<SeedInfo>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct KeyFooter {
    #[serde(rename = "_sha256")]
    sha256: Sha256,
    footer: Binary,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BootBlocks {
    #[serde(rename = "_sha256")]
    sha256: Sha256,
    xz: Binary,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    #[serde(rename = "_sha256")]
    sha256: Sha256,
    fields: ConsoleSettings,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SeedInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bootmii_boot2: Option<BootMiiState>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    footer_header: Option<String>,
}

#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum BootMiiState {
    Detected,
    Stock,
    Unknown,
}

fn sha256(bytes: &[u8]) -> String {
    let digest = Sha256Hasher::digest(bytes);
    let mut hex = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    hex
}

fn digest(bytes: &[u8]) -> Sha256 {
    Sha256::try_from(sha256(bytes)).expect("SHA-256 output always has 64 lowercase hex digits")
}

fn decode_xz(compressed: &[u8]) -> Result<Vec<u8>> {
    let memory_limit_kb = u32::try_from(MAX_XZ_MEMORY / 1024).expect("memory limit fits u32");
    let mut decoder = XzReader::new_mem_limit(compressed, false, memory_limit_kb);
    let mut output = Vec::with_capacity(BOOT_BYTES);
    decoder
        .by_ref()
        .take((BOOT_BYTES + 1) as u64)
        .read_to_end(&mut output)
        .context("cannot decompress boot data")?;
    ensure!(
        output.len() == BOOT_BYTES,
        "Boot block data has the wrong size"
    );
    ensure!(
        decoder.into_inner().is_empty(),
        "Boot data contains trailing compressed bytes"
    );
    Ok(output)
}

fn encode_xz(boot: &[u8]) -> Result<Vec<u8>> {
    use std::io::Write;
    ensure!(
        boot.len() == BOOT_BYTES,
        "Boot block data has the wrong size"
    );
    let mut writer = lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::with_preset(1))?;
    writer.write_all(boot)?;
    Ok(writer.finish()?)
}

/// Serialize authenticated native extraction data in the stable plaintext seed format.
pub fn serialize_seed(
    keys: Vec<u8>,
    boot_blocks: Vec<u8>,
    bad_blocks: Vec<u16>,
    settings: ConsoleSettings,
    settings_sha256: [u8; 32],
    bootmii: &str,
    footer_header: &str,
) -> Result<String> {
    ensure!(
        keys.len() == KEYS_BYTES,
        "Seed keys footer has the wrong size"
    );
    ensure!(
        boot_blocks.len() == BOOT_BYTES,
        "Boot block data has the wrong size"
    );
    let bootmii_boot2 = match bootmii {
        "detected" => BootMiiState::Detected,
        "stock" => BootMiiState::Stock,
        "unknown" => BootMiiState::Unknown,
        _ => {
            return Err(anyhow!(
                "Native extractor returned an invalid BootMii state"
            ));
        }
    };
    let manifest = SeedManifest {
        keys: KeyFooter {
            sha256: digest(&keys),
            footer: Binary(keys),
        },
        bad_blocks,
        boot_blocks: BootBlocks {
            sha256: digest(&boot_blocks),
            xz: Binary(encode_xz(&boot_blocks)?),
        },
        settings: Settings {
            sha256: Sha256::try_from(hex_digest(&settings_sha256))?,
            fields: settings,
        },
        info: Some(SeedInfo {
            bootmii_boot2: Some(bootmii_boot2),
            footer_header: (!footer_header.is_empty()).then(|| footer_header.to_owned()),
        }),
    };
    serde_saphyr::to_string(&manifest).context("cannot serialize Wii seed YAML")
}

fn hex_digest(bytes: &[u8; 32]) -> String {
    let mut hex = String::with_capacity(64);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(hex, "{byte:02x}").expect("writing to String cannot fail");
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn settings() -> Result<ConsoleSettings> {
        Ok(ConsoleSettings {
            area: ConsoleSettingValue::try_from("EUR".to_owned())?,
            model: ConsoleSettingValue::try_from("RVL-001(EUR)".to_owned())?,
            dvd: ConsoleSettingValue::try_from("0".to_owned())?,
            mpch: ConsoleSettingValue::try_from("0x7FFE".to_owned())?,
            code: ConsoleSettingValue::try_from("LEH".to_owned())?,
            serial: ConsoleSettingValue::try_from("182060878".to_owned())?,
            video: ConsoleSettingValue::try_from("PAL".to_owned())?,
            game: ConsoleSettingValue::try_from("EU".to_owned())?,
        })
    }

    fn synthetic_seed() -> Result<String> {
        serialize_seed(
            vec![0; KEYS_BYTES],
            vec![0x5a; BOOT_BYTES],
            vec![9],
            settings()?,
            [0; 32],
            "unknown",
            "BOOTMII",
        )
    }

    #[test]
    fn serializes_binary_and_parses_seed_from_reader() -> Result<()> {
        let yaml = synthetic_seed()?;
        assert!(yaml.contains("!!binary"));
        let seed = StockSeed::from_reader(Cursor::new(yaml.as_bytes()))?;
        assert_eq!(seed.keys().len(), KEYS_BYTES);
        assert_eq!(seed.boot_blocks().len(), BOOT_BYTES);
        assert_eq!(seed.bad_blocks(), [9]);
        assert_eq!(seed.settings().area.as_str(), "EUR");
        assert_eq!(seed.settings_sha256(), &[0; 32]);
        Ok(())
    }

    #[test]
    fn hides_parser_excerpts() -> Result<()> {
        let error = StockSeed::parse(b"keys: SECRET_SENTINEL: {").unwrap_err();
        assert_eq!(format!("{error:#}"), "Invalid Wii seed YAML");
        assert!(!format!("{error:#}").contains("SECRET_SENTINEL"));
        Ok(())
    }
}
