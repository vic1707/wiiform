// SPDX-License-Identifier: GPL-2.0-only
//! Validated, decoded inputs from one Wii seed.

use std::io::{Read, Write};

use anyhow::{Context, Result, anyhow, ensure};
use lzma_rust2::XzReader;
use serde::{Deserialize, Serialize};
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
    bootmii: Bootmii,
}

/// What the seed knows about BootMii on its console.
#[derive(Clone, Debug, Default)]
pub struct Bootmii {
    /// BootMii as IOS, as the backup held it.
    pub ios: Option<BootmiiIos>,
    /// Whether the seed noted BootMii as boot2 (a note, not a checked fact).
    pub boot2: bool,
    /// Whether the seed says if the backup had BootMii as IOS. Seeds written
    /// before wii-seed looked for it say nothing.
    pub ios_recorded: bool,
}

/// The IOS254 title HackMii installs for BootMii as IOS.
#[derive(Clone, Debug)]
pub struct BootmiiIos {
    pub tmd: Vec<u8>,
    pub ticket: Vec<u8>,
    pub content: Vec<u8>,
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

        let ios = match manifest.bootmii_ios {
            Some(section) => {
                ensure!(
                    section.digest() == section.sha256,
                    "BootMii as IOS does not match its SHA-256"
                );
                Some(BootmiiIos {
                    tmd: section.tmd.0,
                    ticket: section.ticket.0,
                    content: section.content.0,
                })
            }
            None => None,
        };
        let info = manifest.info.as_ref();
        let boot2 = (info.and_then(|info| info.bootmii_boot2.as_deref()))
            .is_some_and(|state| state == "detected");
        let ios_recorded = ios.is_some() || info.is_some_and(|info| info.bootmii_ios.is_some());

        Ok(Self {
            keys,
            boot_blocks,
            bad_blocks: manifest.bad_blocks,
            settings: manifest.settings.fields,
            settings_sha256: manifest.settings.sha256.0,
            bootmii: Bootmii {
                ios,
                boot2,
                ios_recorded,
            },
        })
    }

    pub fn bootmii(&self) -> &Bootmii {
        &self.bootmii
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
    /// BootMii as IOS (IOS254), when the backup had it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bootmii_ios: Option<BootmiiIosSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    info: Option<SeedInfo>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BootmiiIosSection {
    /// Over the TMD, the ticket and the content, in that order.
    #[serde(rename = "_sha256")]
    sha256: Sha256,
    tmd: Binary,
    ticket: Binary,
    content: Binary,
}

impl BootmiiIosSection {
    fn new(tmd: Vec<u8>, ticket: Vec<u8>, content: Vec<u8>) -> Self {
        let mut section = Self {
            sha256: Sha256([0; 32]),
            tmd: Binary(tmd),
            ticket: Binary(ticket),
            content: Binary(content),
        };
        section.sha256 = section.digest();
        section
    }

    fn digest(&self) -> Sha256 {
        digest([&self.tmd.0[..], &self.ticket.0, &self.content.0].concat())
    }
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
    /// BootMii as IOS: its version, or "none".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bootmii_ios: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    footer_header: Option<String>,
}

fn digest(bytes: impl AsRef<[u8]>) -> Sha256 {
    Sha256::of(bytes.as_ref())
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
    let bootmii_ios_note = match parts.bootmii_ios_tmd.get(0x1dc..0x1de) {
        Some(version) => format!("v{}", u16::from_be_bytes([version[0], version[1]])),
        None => "none".to_owned(),
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
        bootmii_ios: (!parts.bootmii_ios_tmd.is_empty()).then(|| {
            BootmiiIosSection::new(
                parts.bootmii_ios_tmd,
                parts.bootmii_ios_ticket,
                parts.bootmii_ios_content,
            )
        }),
        info: Some(SeedInfo {
            bootmii_boot2: Some(bootmii.to_owned()),
            bootmii_ios: Some(bootmii_ios_note),
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

    fn manifest(
        bootmii_ios: Option<BootmiiIosSection>,
        ios_note: Option<&str>,
    ) -> Result<SeedManifest> {
        Ok(SeedManifest {
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
            bootmii_ios,
            info: Some(SeedInfo {
                bootmii_boot2: Some("detected".into()),
                bootmii_ios: ios_note.map(str::to_owned),
                footer_header: None,
            }),
        })
    }

    fn read(manifest: &SeedManifest) -> Result<StockSeed> {
        let mut yaml = Vec::new();
        serde_saphyr::to_io_writer(&mut yaml, manifest)?;
        StockSeed::from_reader(Cursor::new(yaml))
    }

    #[test]
    fn decodes_binary_and_reads_settings_through_native_type() -> Result<()> {
        let seed = read(&manifest(None, None)?)?;
        assert_eq!(seed.keys.len(), KEYS_BYTES);
        assert_eq!(seed.boot_blocks.len(), BOOT_BYTES);
        assert_eq!(seed.bad_blocks, [9]);
        assert_eq!(seed.settings.area, "EUR");
        assert_eq!(seed.settings_sha256, [0; 32]);
        let bootmii = seed.bootmii();
        assert!(
            bootmii.ios.is_none() && bootmii.boot2 && !bootmii.ios_recorded,
            "an older seed says nothing"
        );
        assert!(read(&manifest(None, Some("none"))?)?.bootmii().ios_recorded);
        Ok(())
    }

    #[test]
    fn carries_bootmii_as_ios_with_its_digest() -> Result<()> {
        let section =
            || BootmiiIosSection::new(b"tmd".to_vec(), b"ticket".to_vec(), b"content".to_vec());
        let seed = read(&manifest(Some(section()), Some("v65281"))?)?;
        let ios = seed
            .bootmii()
            .ios
            .as_ref()
            .context("expected BootMii as IOS")?;
        assert_eq!(
            (&ios.tmd[..], &ios.ticket[..], &ios.content[..]),
            (&b"tmd"[..], &b"ticket"[..], &b"content"[..])
        );

        let mut tampered = section();
        tampered.content = Binary(b"other".to_vec());
        let Err(error) = read(&manifest(Some(tampered), Some("v65281"))?) else {
            panic!("a changed BootMii content must be refused");
        };
        assert!(format!("{error:#}").contains("BootMii as IOS does not match its SHA-256"));
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
