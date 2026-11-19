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
#[derive(Clone, Debug)]
pub struct Bootmii {
    /// What the backup's boot area showed. A heuristic, not a checked fact.
    pub boot2: Boot2,
    /// The marker of the detected boot2 copy, such as `BM1.5`.
    pub boot2_version: Option<String>,
    /// BootMii as IOS, as the backup held it.
    pub ios: Option<BootmiiIos>,
}

/// BootMii as boot2, as the backup's boot area shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Boot2 {
    Detected,
    Stock,
    Unknown,
}

/// The IOS254 title HackMii installs for BootMii as IOS, its digest checked.
#[derive(Clone, Debug)]
pub struct BootmiiIos {
    pub tmd: Vec<u8>,
    pub ticket: Vec<u8>,
    pub content: Vec<u8>,
}

/// The TMD's reserved text, where BootMii as IOS writes its marker.
const TMD_MARKER: std::ops::Range<usize> = 0x1b2..0x1b7;

/// A BootMii marker such as `BM1.5`: `BM`, a digit, a dot and a digit.
fn marker_version(bytes: &[u8]) -> Option<String> {
    let marker = bytes.get(..5)?;
    let valid = &marker[..2] == b"BM"
        && marker[2].is_ascii_digit()
        && marker[3] == b'.'
        && marker[4].is_ascii_digit();
    valid.then(|| String::from_utf8_lossy(marker).into_owned())
}

fn is_marker(version: &str) -> bool {
    marker_version(version.as_bytes()).as_deref() == Some(version)
}

impl BootmiiIos {
    fn version(&self) -> Option<String> {
        self.tmd.get(TMD_MARKER).and_then(marker_version)
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
        let manifest: SeedManifest = serde_saphyr::from_reader_with_options(
            reader,
            crate::recipe::yaml::options(),
        )
        .map_err(|_| {
            anyhow!(
                "Invalid Wii seed YAML (a seed from before the grouped keys must be regenerated)"
            )
        })?;

        let mut keys = manifest.keys.join()?;
        // The header is never stored; wiiform writes its own on every build.
        native::write_backupmii_header(&mut keys)?;

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
            bootmii: manifest.bootmii.try_into()?,
        })
    }

    pub fn bootmii(&self) -> &Bootmii {
        &self.bootmii
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
    bootmii: BootmiiSection,
}

/// The key footer, split into groups that each change for one reason. Each
/// group's digest moves only when its own bytes do.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct KeyFooter {
    /// Identifiers and the boot1 hash: fixed for a console.
    console: KeyGroup,
    /// Counters that move with use, kept as dumped.
    counters: KeyGroup,
    /// Key material: fixed for a console.
    secret: KeyGroup,
    /// Nonzero bytes outside the groups and the header; empty for a known console.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    other: Vec<KeyRange>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct KeyGroup {
    #[serde(rename = "_sha256")]
    sha256: Sha256,
    blob: Binary,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct KeyRange {
    offset: usize,
    blob: Binary,
}

/// Where each group sits in the 1024-byte key footer, as (offset, length) regions
/// whose bytes make up the group's blob in this order.
mod footer {
    use std::ops::Range;

    pub const CONSOLE: &[(usize, usize)] = &[
        (0x100, 20), // boot1 SHA-1
        (0x124, 4),  // console ID
        (0x200, 4),  // ms_id
        (0x204, 4),  // ca_id
        (0x208, 4),  // ng_key_id
        (0x20c, 60), // ng_signature
    ];
    pub const COUNTERS: &[(usize, usize)] = &[
        (0x248, 20), // boot2 counters
        (0x25c, 18), // NAND counters
        (0x2f8, 4),  // prng_seed, bumped at each IOS start
    ];
    pub const SECRET: &[(usize, usize)] = &[
        (0x114, 16), // common key
        (0x128, 30), // ECC private key
        (0x144, 20), // NAND HMAC key
        (0x158, 16), // NAND AES key
        (0x168, 16), // RNG key
        (0x178, 8),  // unknown
        (0x274, 16), // Korean key
    ];
    /// The text header, rewritten on every build.
    pub const HEADER: Range<usize> = 0..0x100;

    pub fn grouped() -> impl Iterator<Item = (usize, usize)> {
        CONSOLE.iter().chain(COUNTERS).chain(SECRET).copied()
    }
}

/// Which bytes of the key footer a set of regions covers.
fn covered(regions: impl Iterator<Item = (usize, usize)>) -> [bool; KEYS_BYTES] {
    let mut covered = [false; KEYS_BYTES];
    for (offset, length) in regions {
        covered[offset..offset + length].fill(true);
    }
    covered
}

fn group_blob(footer: &[u8], regions: &[(usize, usize)]) -> Vec<u8> {
    regions
        .iter()
        .flat_map(|&(offset, length)| &footer[offset..offset + length])
        .copied()
        .collect()
}

fn group(footer: &[u8], regions: &[(usize, usize)]) -> KeyGroup {
    let blob = group_blob(footer, regions);
    KeyGroup {
        sha256: digest(&blob),
        blob: Binary(blob),
    }
}

fn fill_group(
    footer: &mut [u8; KEYS_BYTES],
    regions: &[(usize, usize)],
    group: &KeyGroup,
    name: &str,
) -> Result<()> {
    ensure!(
        digest(&group.blob.0) == group.sha256,
        "{name} keys do not match their SHA-256"
    );
    let length: usize = regions.iter().map(|&(_, length)| length).sum();
    ensure!(
        group.blob.0.len() == length,
        "{name} keys have the wrong size"
    );
    let mut rest = &group.blob.0[..];
    for &(offset, length) in regions {
        let (bytes, tail) = rest.split_at(length);
        footer[offset..offset + length].copy_from_slice(bytes);
        rest = tail;
    }
    Ok(())
}

impl KeyFooter {
    fn split(footer: &[u8; KEYS_BYTES]) -> Self {
        let claimed =
            covered(footer::grouped().chain([(footer::HEADER.start, footer::HEADER.len())]));
        let mut other = Vec::new();
        let mut at = 0;
        while at < KEYS_BYTES {
            if claimed[at] || footer[at] == 0 {
                at += 1;
                continue;
            }
            let offset = at;
            while at < KEYS_BYTES && !claimed[at] && footer[at] != 0 {
                at += 1;
            }
            other.push(KeyRange {
                offset,
                blob: Binary(footer[offset..at].to_vec()),
            });
        }
        Self {
            console: group(footer, footer::CONSOLE),
            counters: group(footer, footer::COUNTERS),
            secret: group(footer, footer::SECRET),
            other,
        }
    }

    /// Rebuilds the footer with a zeroed header, checking every digest and range.
    fn join(&self) -> Result<Box<[u8; KEYS_BYTES]>> {
        let mut footer = Box::new([0; KEYS_BYTES]);
        fill_group(&mut footer, footer::CONSOLE, &self.console, "Console")?;
        fill_group(&mut footer, footer::COUNTERS, &self.counters, "Counters")?;
        fill_group(&mut footer, footer::SECRET, &self.secret, "Secret")?;
        let mut taken = covered(footer::grouped());
        for range in &self.other {
            let end = range
                .offset
                .checked_add(range.blob.0.len())
                .filter(|&end| end <= KEYS_BYTES)
                .filter(|_| !range.blob.0.is_empty())
                .ok_or_else(|| anyhow!("A key range lies outside the key footer"))?;
            ensure!(
                range.offset >= footer::HEADER.end && !taken[range.offset..end].contains(&true),
                "Key ranges overlap each other or the key groups"
            );
            footer[range.offset..end].copy_from_slice(&range.blob.0);
            taken[range.offset..end].fill(true);
        }
        Ok(footer)
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BootmiiSection {
    boot2: Boot2Section,
    /// Always written, as `null` when the backup has no BootMii as IOS.
    #[serde(deserialize_with = "Option::deserialize")]
    ios: Option<BootmiiIosSection>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Boot2Section {
    state: Boot2,
    /// The marker of the detected boot2 copy; absent when not detected.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<String>,
}

impl TryFrom<BootmiiSection> for Bootmii {
    type Error = anyhow::Error;

    fn try_from(section: BootmiiSection) -> Result<Self> {
        if let Some(version) = &section.boot2.version {
            ensure!(
                is_marker(version),
                "boot2 version {version} is not a BootMii marker"
            );
        }
        Ok(Self {
            boot2: section.boot2.state,
            boot2_version: section.boot2.version,
            ios: section.ios.map(BootmiiIos::try_from).transpose()?,
        })
    }
}

impl From<Bootmii> for BootmiiSection {
    fn from(bootmii: Bootmii) -> Self {
        Self {
            boot2: Boot2Section {
                state: bootmii.boot2,
                version: bootmii.boot2_version,
            },
            ios: bootmii.ios.map(BootmiiIosSection::from),
        }
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct BootmiiIosSection {
    /// The marker in the TMD, such as `BM1.5`; a new one means a new BootMii as IOS.
    version: String,
    /// Over the TMD, the ticket and the content, in that order.
    #[serde(rename = "_sha256")]
    sha256: Sha256,
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
            ios.version().as_deref() == Some(section.version.as_str()),
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
            // A TMD without a marker is refused when read back.
            version: ios.version().unwrap_or_default(),
            tmd: Binary(ios.tmd),
            ticket: Binary(ios.ticket),
            content: Binary(ios.content),
        }
    }
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
    let keys: [u8; KEYS_BYTES] = parts
        .keys
        .try_into()
        .map_err(|_| anyhow!("Expected a 1024-byte key footer"))?;
    let boot2 = match parts.bootmii {
        ffi::BootMiiState::Detected => Boot2::Detected,
        ffi::BootMiiState::Stock => Boot2::Stock,
        _ => Boot2::Unknown,
    };
    let manifest = SeedManifest {
        keys: KeyFooter::split(&keys),
        bad_blocks: parts.bad_blocks,
        boot_blocks: BootBlocks {
            sha256: digest(&parts.boot_blocks),
            xz: Binary(encode_xz(&parts.boot_blocks)?),
        },
        settings: Settings {
            sha256: Sha256(parts.settings_sha256),
            fields: parts.settings,
        },
        bootmii: Bootmii {
            boot2,
            boot2_version: (!parts.bootmii_boot2_version.is_empty())
                .then_some(parts.bootmii_boot2_version),
            ios: (!parts.bootmii_ios_tmd.is_empty()).then_some(BootmiiIos {
                tmd: parts.bootmii_ios_tmd,
                ticket: parts.bootmii_ios_ticket,
                content: parts.bootmii_ios_content,
            }),
        }
        .into(),
    };
    serde_saphyr::to_io_writer(&mut writer, &manifest).context("cannot serialize Wii seed YAML")?;
    writer.flush().context("cannot write Wii seed YAML")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// A footer with a nonzero value in every grouped byte, a header, and one
    /// unknown byte; everything else is zero, as on a real console.
    fn sample_footer() -> [u8; KEYS_BYTES] {
        let mut footer = [0u8; KEYS_BYTES];
        footer[..16].copy_from_slice(b"BackupMii v1, C\n");
        let mut value = 1u8;
        for (offset, length) in footer::grouped() {
            for byte in &mut footer[offset..offset + length] {
                *byte = value | 1; // odd, so never zero
                value = value.wrapping_add(37);
            }
        }
        footer[0x300] = 0x7a;
        footer
    }

    fn manifest(bootmii: Bootmii) -> Result<SeedManifest> {
        Ok(SeedManifest {
            keys: KeyFooter::split(&sample_footer()),
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
            bootmii: bootmii.into(),
        })
    }

    fn stock_bootmii() -> Bootmii {
        Bootmii {
            boot2: Boot2::Stock,
            boot2_version: None,
            ios: None,
        }
    }

    fn yaml(manifest: &SeedManifest) -> Result<String> {
        Ok(serde_saphyr::to_string(manifest)?)
    }

    fn read(yaml: &str) -> Result<StockSeed> {
        StockSeed::from_reader(Cursor::new(yaml))
    }

    /// A TMD long enough to hold its marker, which BootMii writes at 0x1b2.
    fn bootmii_ios(marker: &str) -> BootmiiIos {
        let mut tmd = vec![0; 0x1e4];
        tmd[TMD_MARKER].copy_from_slice(marker.as_bytes());
        BootmiiIos {
            tmd,
            ticket: b"ticket".to_vec(),
            content: b"content".to_vec(),
        }
    }

    #[test]
    fn splits_the_footer_into_groups_and_rebuilds_it() -> Result<()> {
        let source = sample_footer();
        let split = KeyFooter::split(&source);
        assert_eq!(split.other.len(), 1, "the one unknown byte");
        assert_eq!(split.other[0].offset, 0x300);

        let seed = read(&yaml(&manifest(stock_bootmii())?)?)?;
        assert_eq!(
            seed.keys[0x100..],
            source[0x100..],
            "every group and range comes back"
        );
        assert!(
            seed.keys.starts_with(b"BackupMii v1, "),
            "the header is written on read"
        );
        assert_eq!(seed.boot_blocks.len(), BOOT_BYTES);
        assert_eq!(seed.bad_blocks, [9]);
        assert_eq!(seed.settings.area, "EUR");
        assert_eq!(seed.bootmii().boot2, Boot2::Stock);
        Ok(())
    }

    #[test]
    fn a_group_moves_its_own_digest_only() {
        let before = KeyFooter::split(&sample_footer());
        let mut moved = sample_footer();
        moved[0x2f8] ^= 0xff; // prng_seed, a counter
        let after = KeyFooter::split(&moved);
        assert_eq!(before.console.sha256, after.console.sha256);
        assert_eq!(before.secret.sha256, after.secret.sha256);
        assert_ne!(before.counters.sha256, after.counters.sha256);
    }

    #[test]
    fn refuses_a_group_that_does_not_match_its_digest() -> Result<()> {
        let mut seed = manifest(stock_bootmii())?;
        seed.keys.console.blob.0[0] ^= 1;
        let error = read(&yaml(&seed)?).err().context("expected a refusal")?;
        assert_eq!(
            format!("{error:#}"),
            "Console keys do not match their SHA-256"
        );
        Ok(())
    }

    #[test]
    fn refuses_key_ranges_that_overlap_or_leave_the_footer() -> Result<()> {
        let refusal = |edit: fn(&mut KeyFooter)| -> Result<String> {
            let mut seed = manifest(stock_bootmii())?;
            edit(&mut seed.keys);
            let error = read(&yaml(&seed)?).err().context("expected a refusal")?;
            Ok(format!("{error:#}"))
        };
        assert!(
            refusal(|keys| keys.other[0].offset = 0x100)?
                .contains("overlap each other or the key groups")
        );
        assert!(refusal(|keys| keys.other[0].offset = 0x400)?.contains("outside the key footer"));
        Ok(())
    }

    #[test]
    fn refuses_the_old_single_footer_seed() -> Result<()> {
        let old =
            yaml(&manifest(stock_bootmii())?)?.replace("  console:", "  footer: AAAA\n  console:");
        let error = read(&old).err().context("expected a refusal")?;
        assert!(format!("{error:#}").contains("must be regenerated"));
        Ok(())
    }

    #[test]
    fn states_a_missing_bootmii_as_ios() -> Result<()> {
        let written = yaml(&manifest(stock_bootmii())?)?;
        assert!(written.contains("state: stock"), "{written}");
        assert!(written.contains("ios: null"), "{written}");
        let bootmii = read(&written)?;
        assert!(bootmii.bootmii().boot2 == Boot2::Stock && bootmii.bootmii().ios.is_none());

        let unstated = written.replace("  ios: null\n", "");
        assert!(read(&unstated).is_err(), "ios must be stated");
        Ok(())
    }

    #[test]
    fn carries_boot2_and_bootmii_as_ios_by_their_markers() -> Result<()> {
        let detected = |ios| Bootmii {
            boot2: Boot2::Detected,
            boot2_version: Some("BM1.5".into()),
            ios: Some(ios),
        };
        let written = yaml(&manifest(detected(bootmii_ios("BM1.5")))?)?;
        assert!(written.contains("version: BM1.5"), "{written}");
        let seed = read(&written)?;
        let read_back = seed.bootmii();
        let ios = read_back.ios.as_ref().context("expected BootMii as IOS")?;
        assert_eq!(read_back.boot2, Boot2::Detected);
        assert_eq!(read_back.boot2_version.as_deref(), Some("BM1.5"));
        assert_eq!(ios.tmd, bootmii_ios("BM1.5").tmd);
        assert_eq!(
            (&ios.ticket[..], &ios.content[..]),
            (&b"ticket"[..], &b"content"[..])
        );

        let refusal = |edit: fn(&mut BootmiiIosSection)| -> Result<String> {
            let mut section = BootmiiIosSection::from(bootmii_ios("BM1.5"));
            edit(&mut section);
            let mut seed = manifest(stock_bootmii())?;
            seed.bootmii = BootmiiSection {
                boot2: Boot2Section {
                    state: Boot2::Detected,
                    version: None,
                },
                ios: Some(section),
            };
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
            refusal(|section| section.version = "BM1.3".into())?,
            "BootMii as IOS is not version BM1.3"
        );
        let short = BootmiiIosSection::from(BootmiiIos {
            tmd: b"tmd".to_vec(),
            ..bootmii_ios("BM1.5")
        });
        assert!(
            BootmiiIos::try_from(short).is_err(),
            "a TMD without a marker"
        );
        Ok(())
    }

    #[test]
    fn recognizes_only_bootmii_markers() {
        assert_eq!(marker_version(b"BM1.5").as_deref(), Some("BM1.5"));
        assert_eq!(marker_version(b"BM1.5 trailing").as_deref(), Some("BM1.5"));
        assert_eq!(marker_version(b"BM1.x"), None);
        assert_eq!(marker_version(b"XM1.5"), None);
        assert_eq!(marker_version(b"BM1"), None);
        assert!(is_marker("BM1.3") && !is_marker("BM1.35"));
    }

    #[test]
    fn hides_parser_excerpts() {
        let Err(error) = StockSeed::from_reader(&b"keys: SECRET_SENTINEL: {"[..]) else {
            panic!("invalid YAML must be rejected");
        };
        assert!(format!("{error:#}").starts_with("Invalid Wii seed YAML"));
        assert!(!format!("{error:#}").contains("SECRET_SENTINEL"));
    }
}
