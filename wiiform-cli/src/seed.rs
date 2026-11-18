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
    /// None for a seed written before wii-seed looked for BootMii.
    bootmii: Option<Bootmii>,
}

/// What the seed knows about BootMii on its console.
#[derive(Clone, Debug)]
pub struct Bootmii {
    /// What the backup's boot area showed.
    pub boot2: Boot2,
    /// BootMii as IOS, as the backup held it.
    pub ios: Option<BootmiiIos>,
}

/// BootMii as boot2, as the backup's boot area shows it. A heuristic, not a
/// checked fact.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Boot2 {
    Detected,
    Stock,
    Unknown,
}

/// The IOS254 title HackMii installs for BootMii as IOS, its digest and
/// version checked.
#[derive(Clone, Debug)]
pub struct BootmiiIos {
    pub tmd: Vec<u8>,
    pub ticket: Vec<u8>,
    pub content: Vec<u8>,
}

/// The TMD's title version.
const TMD_VERSION: std::ops::Range<usize> = 0x1dc..0x1de;

impl BootmiiIos {
    fn version(&self) -> Option<u16> {
        let bytes = self.tmd.get(TMD_VERSION)?;
        Some(u16::from_be_bytes([bytes[0], bytes[1]]))
    }
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
            bootmii: manifest.bootmii.map(Bootmii::try_from).transpose()?,
        })
    }

    /// None for a seed written before wii-seed looked for BootMii.
    pub fn bootmii(&self) -> Option<&Bootmii> {
        self.bootmii.as_ref()
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
    /// Absent from seeds written before wii-seed looked for BootMii.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bootmii: Option<BootmiiSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    info: Option<SeedInfo>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BootmiiSection {
    boot2: Boot2,
    /// Always written, as `null` when the backup has no BootMii as IOS.
    #[serde(deserialize_with = "Option::deserialize")]
    ios: Option<BootmiiIosSection>,
}

impl TryFrom<BootmiiSection> for Bootmii {
    type Error = anyhow::Error;

    fn try_from(section: BootmiiSection) -> Result<Self> {
        Ok(Self {
            boot2: section.boot2,
            ios: section.ios.map(BootmiiIos::try_from).transpose()?,
        })
    }
}

impl From<Bootmii> for BootmiiSection {
    fn from(bootmii: Bootmii) -> Self {
        Self {
            boot2: bootmii.boot2,
            ios: bootmii.ios.map(BootmiiIosSection::from),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BootmiiIosSection {
    /// Over the TMD, the ticket and the content, in that order.
    #[serde(rename = "_sha256")]
    sha256: Sha256,
    /// The TMD's, repeated for the reader.
    version: u16,
    tmd: Binary,
    ticket: Binary,
    content: Binary,
}

fn bootmii_ios_digest(tmd: &[u8], ticket: &[u8], content: &[u8]) -> Sha256 {
    digest([tmd, ticket, content].concat())
}

impl TryFrom<BootmiiIosSection> for BootmiiIos {
    type Error = anyhow::Error;

    fn try_from(section: BootmiiIosSection) -> Result<Self> {
        let ios = Self {
            tmd: section.tmd.0,
            ticket: section.ticket.0,
            content: section.content.0,
        };
        ensure!(
            bootmii_ios_digest(&ios.tmd, &ios.ticket, &ios.content) == section.sha256,
            "BootMii as IOS does not match its SHA-256"
        );
        ensure!(
            ios.version() == Some(section.version),
            "BootMii as IOS is not version {}",
            section.version
        );
        Ok(ios)
    }
}

impl From<BootmiiIos> for BootmiiIosSection {
    fn from(ios: BootmiiIos) -> Self {
        Self {
            sha256: bootmii_ios_digest(&ios.tmd, &ios.ticket, &ios.content),
            // A TMD too short to hold one is refused when read back.
            version: ios.version().unwrap_or_default(),
            tmd: Binary(ios.tmd),
            ticket: Binary(ios.ticket),
            content: Binary(ios.content),
        }
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
    /// Written by wii-seed before the `bootmii` section; read and ignored.
    #[serde(rename = "bootmii_boot2", default, skip_serializing)]
    _legacy_bootmii_boot2: Option<serde::de::IgnoredAny>,
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
    let boot2 = match parts.bootmii {
        ffi::BootMiiState::Detected => Boot2::Detected,
        ffi::BootMiiState::Stock => Boot2::Stock,
        _ => Boot2::Unknown,
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
        bootmii: Some(
            Bootmii {
                boot2,
                ios: (!parts.bootmii_ios_tmd.is_empty()).then_some(BootmiiIos {
                    tmd: parts.bootmii_ios_tmd,
                    ticket: parts.bootmii_ios_ticket,
                    content: parts.bootmii_ios_content,
                }),
            }
            .into(),
        ),
        info: (!parts.footer_header.is_empty()).then_some(SeedInfo {
            _legacy_bootmii_boot2: None,
            footer_header: Some(parts.footer_header),
        }),
    };
    serde_saphyr::to_io_writer(&mut writer, &manifest).context("cannot serialize Wii seed YAML")?;
    writer.flush().context("cannot write Wii seed YAML")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn manifest(bootmii: Option<Bootmii>) -> Result<SeedManifest> {
        let bootmii = bootmii.map(BootmiiSection::from);
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
            bootmii,
            info: None,
        })
    }

    fn yaml(manifest: &SeedManifest) -> Result<String> {
        Ok(serde_saphyr::to_string(manifest)?)
    }

    fn read(yaml: &str) -> Result<StockSeed> {
        StockSeed::from_reader(Cursor::new(yaml))
    }

    /// A TMD long enough to hold `version`.
    fn bootmii_ios(version: u16) -> BootmiiIos {
        let mut tmd = vec![0; 0x1e4];
        tmd[TMD_VERSION].copy_from_slice(&version.to_be_bytes());
        BootmiiIos {
            tmd,
            ticket: b"ticket".to_vec(),
            content: b"content".to_vec(),
        }
    }

    #[test]
    fn decodes_binary_and_reads_settings_through_native_type() -> Result<()> {
        let seed = read(&yaml(&manifest(None)?)?)?;
        assert_eq!(seed.keys.len(), KEYS_BYTES);
        assert_eq!(seed.boot_blocks.len(), BOOT_BYTES);
        assert_eq!(seed.bad_blocks, [9]);
        assert_eq!(seed.settings.area, "EUR");
        assert_eq!(seed.settings_sha256, [0; 32]);
        assert!(seed.bootmii().is_none(), "an older seed says nothing");
        Ok(())
    }

    #[test]
    fn reads_older_seeds_with_their_boot2_note() -> Result<()> {
        let older = yaml(&manifest(None)?)? + "info:\n  bootmii_boot2: detected\n";
        assert!(read(&older)?.bootmii().is_none());
        Ok(())
    }

    #[test]
    fn states_a_missing_bootmii_as_ios() -> Result<()> {
        let none = Bootmii {
            boot2: Boot2::Stock,
            ios: None,
        };
        let written = yaml(&manifest(Some(none))?)?;
        assert!(
            written.contains("bootmii:\n  boot2: stock\n  ios: null\n"),
            "{written}"
        );
        let bootmii = read(&written)?;
        let bootmii = bootmii.bootmii().context("expected a bootmii section")?;
        assert!(bootmii.boot2 == Boot2::Stock && bootmii.ios.is_none());

        let unstated = written.replace("  ios: null\n", "");
        assert!(read(&unstated).is_err(), "ios must be stated");
        Ok(())
    }

    #[test]
    fn carries_bootmii_as_ios_with_its_digest_and_version() -> Result<()> {
        let bootmii = |ios| Bootmii {
            boot2: Boot2::Detected,
            ios: Some(ios),
        };
        let written = yaml(&manifest(Some(bootmii(bootmii_ios(65281))))?)?;
        assert!(written.contains("    version: 65281\n"), "{written}");
        let seed = read(&written)?;
        let read_back = seed.bootmii().context("expected a bootmii section")?;
        let ios = read_back.ios.as_ref().context("expected BootMii as IOS")?;
        assert_eq!(read_back.boot2, Boot2::Detected);
        assert_eq!(ios.tmd, bootmii_ios(65281).tmd);
        assert_eq!(
            (&ios.ticket[..], &ios.content[..]),
            (&b"ticket"[..], &b"content"[..])
        );

        let refusal = |edit: fn(&mut BootmiiIosSection)| -> Result<String> {
            let mut section = BootmiiIosSection::from(bootmii_ios(65281));
            edit(&mut section);
            let mut seed = manifest(None)?;
            seed.bootmii = Some(BootmiiSection {
                boot2: Boot2::Detected,
                ios: Some(section),
            });
            Ok(format!(
                "{:#}",
                read(&yaml(&seed)?).err().context("expected a refusal")?
            ))
        };
        assert_eq!(
            refusal(|section| section.content = Binary(b"other".to_vec()))?,
            "BootMii as IOS does not match its SHA-256"
        );
        assert_eq!(
            refusal(|section| section.version = 31338)?,
            "BootMii as IOS is not version 31338"
        );
        let short = BootmiiIosSection::from(BootmiiIos {
            tmd: b"tmd".to_vec(),
            ..bootmii_ios(0)
        });
        assert!(
            BootmiiIos::try_from(short).is_err(),
            "a TMD without a version"
        );
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
