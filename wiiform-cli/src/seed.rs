// SPDX-License-Identifier: GPL-2.0-only
//! Validated, decoded inputs from one Wii seed.

use std::io::{Read, Write};

use anyhow::{Context, Result, anyhow, ensure};
use lzma_rust2::XzReader;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256 as Sha256Hasher};
use wiiform_recipe::{Sha256, yaml::Binary};

use crate::ffi::bridge::{self as ffi, ConsoleSettings};
use crate::native::{self, BOOT_BYTES, KEYS_BYTES};

const XZ_MEMORY_KIB: u32 = 128 * 1024;

/// Seed components whose binary digests and compressed boot data were checked.
pub struct StockSeed {
    keys: Box<[u8; KEYS_BYTES]>,
    boot_blocks: Box<[u8; BOOT_BYTES]>,
    bad_blocks: Vec<u16>,
    settings: ConsoleSettings,
    settings_sha256: [u8; 32],
}

/// A seed read from a backup, ready to be written as YAML.
pub struct ExtractedSeed(pub(crate) ffi::SeedParts);

impl ExtractedSeed {
    pub fn write<W: Write>(self, writer: W) -> Result<()> {
        write_seed(writer, self.0)
    }
}

impl StockSeed {
    pub fn from_reader<R: Read>(reader: R) -> Result<Self> {
        let manifest: SeedManifest =
            serde_saphyr::from_reader_with_options(reader, crate::recipe::yaml::options())
                .map_err(|_| anyhow!("Invalid Wii seed YAML"))?;

        let keys: Box<[u8; KEYS_BYTES]> = manifest
            .keys
            .footer
            .0
            .into_boxed_slice()
            .try_into()
            .map_err(|_| anyhow!("Seed keys footer has the wrong size"))?;
        ensure!(
            digest(&keys[..]) == manifest.keys.sha256,
            "Keys do not match their SHA-256"
        );

        let boot_blocks: Box<[u8; BOOT_BYTES]> = decode_xz(&manifest.boot_blocks.xz.0)?
            .into_boxed_slice()
            .try_into()
            .map_err(|_| anyhow!("Boot blocks have the wrong size"))?;
        ensure!(
            digest(&boot_blocks[..]) == manifest.boot_blocks.sha256,
            "Boot blocks do not match their SHA-256"
        );

        Ok(Self {
            keys,
            boot_blocks,
            bad_blocks: manifest.bad_blocks,
            settings: manifest.settings.fields,
            settings_sha256: manifest.settings.sha256.0,
        })
    }

    /// Replaces the key footer's text header with the canonical BackupMii one,
    /// which BootMii's restore requires. Returns whether it changed.
    pub fn write_backupmii_header(&mut self) -> Result<bool> {
        native::write_backupmii_header(&mut self.keys)
    }

    pub(crate) fn as_native(&self) -> ffi::StockSeed<'_> {
        ffi::StockSeed {
            keys: &self.keys[..],
            boot_blocks: &self.boot_blocks[..],
            bad_blocks: &self.bad_blocks,
            settings: &self.settings,
            settings_sha256: self.settings_sha256,
        }
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
    #[serde(with = "ConsoleSettingsSerde")]
    fields: ConsoleSettings,
}

/// Serde view of WiiQt's settings value; field names and validation stay native.
#[derive(Deserialize, Serialize)]
#[serde(
    remote = "ffi::ConsoleSettings",
    rename_all = "UPPERCASE",
    deny_unknown_fields
)]
struct ConsoleSettingsSerde {
    area: String,
    model: String,
    dvd: String,
    mpch: String,
    code: String,
    #[serde(rename = "SERNO")]
    serial: String,
    video: String,
    game: String,
}

/// Notes for the reader of a seed. Builds do not use them.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct SeedInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bootmii_boot2: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    footer_header: Option<String>,
}

fn digest(bytes: impl AsRef<[u8]>) -> Sha256 {
    Sha256(Sha256Hasher::digest(bytes).into())
}

fn decode_xz(compressed: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = XzReader::new_mem_limit(compressed, false, XZ_MEMORY_KIB);
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
    let mut writer = lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::with_preset(1))?;
    writer.write_all(boot)?;
    Ok(writer.finish()?)
}

/// Write native extraction data as the stable plaintext seed YAML format.
fn write_seed<W: Write>(mut writer: W, parts: ffi::SeedParts) -> Result<()> {
    let bootmii = match parts.bootmii {
        ffi::BootMiiState::Detected => "detected",
        ffi::BootMiiState::Stock => "stock",
        _ => "unknown",
    };
    let manifest = SeedManifest {
        keys: KeyFooter {
            sha256: digest(&parts.keys),
            footer: Binary(parts.keys),
        },
        bad_blocks: parts.bad_blocks,
        boot_blocks: BootBlocks {
            sha256: digest(&parts.boot_blocks),
            xz: Binary(encode_xz(&parts.boot_blocks)?),
        },
        settings: Settings {
            sha256: Sha256(parts.settings_sha256),
            fields: parts.settings,
        },
        info: Some(SeedInfo {
            bootmii_boot2: Some(bootmii.to_owned()),
            footer_header: (!parts.footer_header.is_empty()).then_some(parts.footer_header),
        }),
    };
    serde_saphyr::to_io_writer(&mut writer, &manifest).context("cannot serialize Wii seed YAML")?;
    writer.flush().context("cannot write Wii seed YAML")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn decodes_binary_and_reads_settings_through_native_type() -> Result<()> {
        let manifest = SeedManifest {
            keys: KeyFooter {
                sha256: digest(vec![0; KEYS_BYTES]),
                footer: Binary(vec![0; KEYS_BYTES]),
            },
            bad_blocks: vec![9],
            boot_blocks: BootBlocks {
                sha256: digest(vec![0x5a; BOOT_BYTES]),
                xz: Binary(encode_xz(&vec![0x5a; BOOT_BYTES])?),
            },
            settings: Settings {
                sha256: Sha256([0; 32]),
                fields: ConsoleSettings {
                    area: "EUR".into(),
                    model: "RVL-001(EUR)".into(),
                    dvd: "0".into(),
                    mpch: "0x7FFE".into(),
                    code: "LEH".into(),
                    serial: "182060878".into(),
                    video: "PAL".into(),
                    game: "EU".into(),
                },
            },
            info: None,
        };
        let mut yaml = Vec::new();
        serde_saphyr::to_io_writer(&mut yaml, &manifest)?;
        assert!(String::from_utf8_lossy(&yaml).contains("!!binary"));
        let seed = StockSeed::from_reader(Cursor::new(yaml))?;
        assert_eq!(seed.keys.len(), KEYS_BYTES);
        assert_eq!(seed.boot_blocks.len(), BOOT_BYTES);
        assert_eq!(seed.bad_blocks, [9]);
        assert_eq!(seed.settings.area, "EUR");
        assert_eq!(seed.settings_sha256, [0; 32]);
        Ok(())
    }

    #[test]
    fn hides_parser_excerpts() {
        let Err(error) = StockSeed::from_reader(&b"keys: SECRET_SENTINEL: {"[..]) else {
            panic!("invalid YAML must be rejected");
        };
        assert_eq!(format!("{error:#}"), "Invalid Wii seed YAML");
        assert!(!format!("{error:#}").contains("SECRET_SENTINEL"));
    }
}
