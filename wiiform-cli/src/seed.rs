// SPDX-License-Identifier: GPL-2.0-only
//! Validated, decoded inputs from one Wii seed.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs::File,
    io::Read,
    path::Path,
};

use anyhow::{Context, Result, anyhow, ensure};
use lzma_rust2::XzReader;
use nnn::nnn;
use serde::{Deserialize, Deserializer, de::Visitor};
use sha2::{Digest, Sha256 as Sha256Hasher};
use wiiform_recipe::Sha256;

const MAX_SEED_BYTES: usize = 2 * 1024 * 1024;
const BOOT_BYTES: usize = 0x108000;
const KEYS_BYTES: usize = 0x400;
const SETTINGS_BYTES: usize = 256;
const MAX_XZ_MEMORY: u64 = 128 * 1024 * 1024;

#[nnn(
    derive(Clone, Eq, PartialEq, serde::Serialize, serde::Deserialize),
    nnn_derive(TryFrom, AsRef),
    validators(custom(with = valid_setting_value, error = InvalidConsoleSettingValue))
)]
pub struct ConsoleSettingValue(String);

fn valid_setting_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 32
        && value.bytes().all(|byte| (0x20..=0x7e).contains(&byte))
}

impl ConsoleSettingValue {
    pub fn as_str(&self) -> &str {
        self.as_ref().as_str()
    }
}

#[derive(Clone, Deserialize, Eq, PartialEq)]
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

impl ConsoleSettings {
    fn ordered_values(&self) -> [(&'static str, &str); 8] {
        [
            ("AREA", self.area.as_str()),
            ("MODEL", self.model.as_str()),
            ("DVD", self.dvd.as_str()),
            ("MPCH", self.mpch.as_str()),
            ("CODE", self.code.as_str()),
            ("SERNO", self.serial.as_str()),
            ("VIDEO", self.video.as_str()),
            ("GAME", self.game.as_str()),
        ]
    }

    fn encode(&self) -> Result<Vec<u8>> {
        let mut plaintext = Vec::with_capacity(SETTINGS_BYTES);
        for (key, value) in self.ordered_values() {
            plaintext.extend_from_slice(key.as_bytes());
            plaintext.push(b'=');
            plaintext.extend_from_slice(value.as_bytes());
            plaintext.extend_from_slice(b"\r\n");
        }
        ensure!(
            plaintext.len() <= SETTINGS_BYTES,
            "Console settings exceed 256 bytes"
        );
        plaintext.resize(SETTINGS_BYTES, 0);
        crypt_settings(plaintext)
    }
}

/// Seed components checked against their embedded digests and one another.
#[derive(Clone, Eq, PartialEq)]
pub struct StockSeed {
    keys: Vec<u8>,
    boot_blocks: Vec<u8>,
    bad_blocks: Vec<u16>,
    setting_txt: Vec<u8>,
    source_settings: ConsoleSettings,
}

impl StockSeed {
    pub fn read(path: &Path) -> Result<Self> {
        let mut file =
            File::open(path).with_context(|| format!("cannot open seed {}", path.display()))?;
        let mut bytes = Vec::with_capacity(64 * 1024);
        file.by_ref()
            .take((MAX_SEED_BYTES + 1) as u64)
            .read_to_end(&mut bytes)
            .context("cannot read seed")?;
        ensure!(bytes.len() <= MAX_SEED_BYTES, "Seed exceeds 2 MiB");
        Self::parse(&bytes)
    }

    pub fn parse(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_SEED_BYTES, "Seed exceeds 2 MiB");
        let text = std::str::from_utf8(bytes).context("Seed YAML must be UTF-8")?;
        let manifest: SeedManifest = serde_saphyr::from_str_with_options(text, seed_yaml_options())
            .map_err(|_| anyhow!("Invalid Wii seed YAML"))?;
        ensure!(
            manifest.sops.is_none(),
            "Decrypt the SOPS seed explicitly before use"
        );
        ensure!(
            manifest.source_sha256.is_none() && manifest.keys_sha256.is_none(),
            "Seed uses obsolete hash fields. Re-export it from the NAND backup"
        );
        let _source_backup_digest = manifest.source.sha256;
        let _boot1_version = manifest.boot1_version;
        let _bootmii_state = manifest.bootmii_boot2;
        for copy in &manifest.boot2 {
            ensure!(
                copy.blocks[0] < 8 && copy.blocks[1] < 8,
                "Boot2 copy has an invalid block number"
            );
            ensure!(
                copy.blocks[0] != copy.blocks[1],
                "Boot2 copy repeats a block number"
            );
            let _copy_state = &copy.state;
            let _version = copy.version;
            let _flags = (copy.used_to_boot, copy.marked_bad);
            if let Some(marker) = &copy.bootmii_marker {
                ensure!(
                    matches!(marker.as_str(), "BM1.1" | "BM1.3" | "BM1.5"),
                    "BootMii marker is invalid"
                );
            }
        }

        let expected_keys_hash = manifest.keys.sha256.clone();
        let keys = manifest.keys.restore()?;
        ensure!(
            sha256(&keys) == expected_keys_hash.as_str(),
            "Keys do not match their SHA-256"
        );

        let boot_blocks = decode_xz(&manifest.boot_blocks.xz.0)?;
        ensure!(
            boot_blocks.len() == BOOT_BYTES,
            "Boot block data has the wrong size"
        );
        ensure!(
            sha256(&boot_blocks) == manifest.boot_blocks.sha256.as_str(),
            "Boot blocks do not match their SHA-256"
        );

        let mut seen = BTreeSet::new();
        let bad_blocks = manifest
            .bad_blocks
            .into_iter()
            .map(|block| {
                ensure!(
                    (8..4080).contains(&block),
                    "Bad block is outside the supported NAND range"
                );
                ensure!(seen.insert(block), "Seed contains a duplicate bad block");
                Ok(block)
            })
            .collect::<Result<Vec<_>>>()?;

        let setting_txt = match manifest.settings.original {
            Some(original) => {
                let bytes = original.0;
                ensure!(
                    bytes.len() == SETTINGS_BYTES,
                    "Original setting.txt has the wrong size"
                );
                ensure!(
                    sha256(&bytes) == manifest.settings.sha256.as_str(),
                    "Original setting.txt does not match its SHA-256"
                );
                ensure!(
                    decode_settings(&bytes)? == manifest.settings.fields,
                    "Original setting.txt fields disagree with the readable seed settings"
                );
                ensure!(
                    manifest.settings.fields.encode()? == bytes,
                    "Stock creation requires canonical console settings"
                );
                bytes
            }
            None => {
                let bytes = manifest.settings.fields.encode()?;
                ensure!(
                    sha256(&bytes) == manifest.settings.sha256.as_str(),
                    "Reconstructed setting.txt does not match its SHA-256"
                );
                bytes
            }
        };

        ensure!(
            manifest
                .settings
                .fields
                .model
                .as_str()
                .starts_with("RVL-001(")
                || manifest
                    .settings
                    .fields
                    .model
                    .as_str()
                    .starts_with("RVL-101("),
            "Unsupported console model"
        );
        ensure!(
            ["EUR)", "USA)", "JPN)", "UAE)"]
                .iter()
                .any(|suffix| manifest.settings.fields.model.as_str().ends_with(suffix)),
            "Unsupported console model region"
        );
        let expected_region = match manifest.settings.fields.area.as_str() {
            "EUR" => Some(("EU", "PAL")),
            "USA" => Some(("US", "NTSC")),
            "JPN" => Some(("JP", "NTSC")),
            _ => None,
        };
        let (expected_game, expected_video) =
            expected_region.context("Unsupported source console region")?;
        ensure!(
            manifest.settings.fields.game.as_str() == expected_game
                && manifest.settings.fields.video.as_str() == expected_video,
            "Source console region settings disagree"
        );
        ensure!(
            !keys[0x274..0x284].iter().copied().eq(KOREAN_KEY),
            "Korean-key consoles are unsupported by the stock profile"
        );

        Ok(Self {
            keys,
            boot_blocks,
            bad_blocks,
            setting_txt,
            source_settings: manifest.settings.fields,
        })
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

    pub fn setting_txt(&self) -> &[u8] {
        &self.setting_txt
    }

    pub fn source_settings(&self) -> &ConsoleSettings {
        &self.source_settings
    }
}

fn seed_yaml_options() -> serde_saphyr::Options {
    let mut options = serde_saphyr::Options::default();
    options.duplicate_keys = serde_saphyr::DuplicateKeyPolicy::Error;
    options.reject_unsupported_tags = true;
    // A seed contains console secrets. Parser errors must never echo YAML
    // source text through anyhow's error chain or a downstream CLI.
    options.with_snippet = false;
    options.crop_radius = 0;
    let mut budget = serde_saphyr::Budget::default();
    budget.max_reader_input_bytes = Some(MAX_SEED_BYTES);
    budget.max_events = 20_000;
    budget.max_aliases = 0;
    budget.max_anchors = 0;
    budget.max_depth = 24;
    budget.max_documents = 1;
    budget.max_nodes = 8_192;
    budget.max_total_scalar_bytes = MAX_SEED_BYTES;
    options.budget = Some(budget);
    options
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SeedManifest {
    source: Source,
    keys: Keys,
    bad_blocks: Vec<u16>,
    boot_blocks: BootBlocks,
    settings: Settings,
    #[serde(default)]
    boot1_version: Option<u8>,
    #[serde(default)]
    boot2: Vec<Boot2Copy>,
    #[serde(default)]
    bootmii_boot2: Option<BootMiiState>,
    #[serde(default)]
    sops: Option<serde_json::Value>,
    #[serde(default, rename = "source_sha256")]
    source_sha256: Option<String>,
    #[serde(default, rename = "keys_sha256")]
    keys_sha256: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    sha256: Sha256,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Keys {
    header: String,
    otp: Otp,
    seeprom: Seeprom,
    extra: Vec<KeyExtra>,
    #[serde(rename = "_sha256")]
    sha256: Sha256,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Otp {
    boot1_sha1: String,
    common_key: String,
    console_id: String,
    ng_private_key: String,
    nand_hmac_key: String,
    nand_aes_key: String,
    rng_key: String,
    unknown_78: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Seeprom {
    ms_id: String,
    ca_id: String,
    ng_key_id: String,
    ng_signature: String,
    boot2_counters: String,
    nand_counters: String,
    korean_key: String,
    prng_seed: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyExtra {
    offset: usize,
    hex: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BootBlocks {
    #[serde(rename = "_sha256")]
    sha256: Sha256,
    xz: Binary,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    #[serde(rename = "_sha256")]
    sha256: Sha256,
    fields: ConsoleSettings,
    #[serde(default)]
    original: Option<Binary>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Boot2Copy {
    blocks: [u16; 2],
    version: u32,
    state: Boot2State,
    #[serde(default)]
    used_to_boot: bool,
    #[serde(default)]
    marked_bad: bool,
    #[serde(default)]
    bootmii_marker: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Boot2State {
    Unknown,
    Invalid,
    Stock,
    Fakesigned,
}

#[derive(Deserialize)]
enum BootMiiState {
    #[serde(rename = "detected")]
    Detected,
    #[serde(rename = "stock")]
    Stock,
    #[serde(rename = "unknown")]
    Unknown,
}

struct Binary(Vec<u8>);

impl<'de> Deserialize<'de> for Binary {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct BinaryVisitor;

        impl<'de> Visitor<'de> for BinaryVisitor {
            type Value = Binary;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a YAML binary value or a byte-preserving string")
            }

            fn visit_byte_buf<E>(self, value: Vec<u8>) -> std::result::Result<Binary, E> {
                Ok(Binary(value))
            }

            fn visit_bytes<E>(self, value: &[u8]) -> std::result::Result<Binary, E> {
                Ok(Binary(value.to_vec()))
            }

            fn visit_str<E>(self, value: &str) -> std::result::Result<Binary, E>
            where
                E: serde::de::Error,
            {
                Ok(Binary(value.as_bytes().to_vec()))
            }

            fn visit_string<E>(self, value: String) -> std::result::Result<Binary, E> {
                Ok(Binary(value.into_bytes()))
            }

            fn visit_seq<A>(self, mut seq: A) -> std::result::Result<Binary, A::Error>
            where
                A: serde::de::SeqAccess<'de>,
            {
                let mut bytes = Vec::new();
                while let Some(byte) = seq.next_element::<u8>()? {
                    bytes.push(byte);
                }
                Ok(Binary(bytes))
            }
        }

        deserializer.deserialize_byte_buf(BinaryVisitor)
    }
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

fn decode_hex(value: &str, expected_len: usize, field: &str) -> Result<Vec<u8>> {
    ensure!(
        value.len() == expected_len * 2,
        "{field} has an invalid hex length"
    );
    ensure!(
        value.bytes().all(|byte| byte.is_ascii_hexdigit()),
        "{field} contains invalid hex"
    );
    let mut bytes = Vec::with_capacity(expected_len);
    for pair in value.as_bytes().as_chunks::<2>().0 {
        let digit = |byte: u8| match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => unreachable!("hex digits were checked"),
        };
        bytes.push(digit(pair[0]) << 4 | digit(pair[1]));
    }
    Ok(bytes)
}

impl Keys {
    fn restore(self) -> Result<Vec<u8>> {
        let header: Vec<u8> = self
            .header
            .chars()
            .map(|character| u8::try_from(character as u32).context("Key header is not Latin-1"))
            .collect::<Result<_>>()?;
        ensure!(header.len() <= 0x100, "Key header exceeds its size limit");
        let mut keys = vec![0; KEYS_BYTES];
        let mut populated = vec![false; KEYS_BYTES];
        keys[..header.len()].copy_from_slice(&header);
        populated[..header.len()].fill(true);

        let fields = [
            (0x100, 20, self.otp.boot1_sha1),
            (0x114, 16, self.otp.common_key),
            (0x124, 4, self.otp.console_id),
            (0x128, 30, self.otp.ng_private_key),
            (0x144, 20, self.otp.nand_hmac_key),
            (0x158, 16, self.otp.nand_aes_key),
            (0x168, 16, self.otp.rng_key),
            (0x178, 8, self.otp.unknown_78),
            (0x200, 4, self.seeprom.ms_id),
            (0x204, 4, self.seeprom.ca_id),
            (0x208, 4, self.seeprom.ng_key_id),
            (0x20c, 60, self.seeprom.ng_signature),
            (0x248, 20, self.seeprom.boot2_counters),
            (0x25c, 18, self.seeprom.nand_counters),
            (0x274, 16, self.seeprom.korean_key),
            (0x2f8, 4, self.seeprom.prng_seed),
        ];
        for (offset, length, text) in fields {
            let bytes = decode_hex(&text, length, "key field")?;
            for (position, byte) in bytes.into_iter().enumerate() {
                let position = offset + position;
                ensure!(
                    !populated[position] || keys[position] == byte,
                    "Key fields overlap with different values"
                );
                keys[position] = byte;
                populated[position] = true;
            }
        }

        for extra in self.extra {
            let bytes = decode_hex(&extra.hex, extra.hex.len() / 2, "extra key field")?;
            ensure!(!bytes.is_empty(), "Extra key fields cannot be empty");
            ensure!(
                bytes.len() <= KEYS_BYTES,
                "Extra key field exceeds keys.bin"
            );
            ensure!(
                extra.offset <= KEYS_BYTES - bytes.len(),
                "Extra key field exceeds keys.bin"
            );
            for (position, byte) in bytes.into_iter().enumerate() {
                let position = extra.offset + position;
                ensure!(
                    !populated[position],
                    "Extra key field overlaps another key field"
                );
                populated[position] = true;
                keys[position] = byte;
            }
        }
        Ok(keys)
    }
}

fn decode_xz(compressed: &[u8]) -> Result<Vec<u8>> {
    // The pinned decoder's single-threaded reader accepts a memory limit.
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
    let mut trailing = [0u8; 1];
    ensure!(
        decoder.read(&mut trailing)? == 0,
        "Boot data expands beyond its expected size"
    );
    ensure!(
        decoder.into_inner().is_empty(),
        "Boot data contains trailing compressed bytes"
    );
    Ok(output)
}

fn crypt_settings(mut bytes: Vec<u8>) -> Result<Vec<u8>> {
    ensure!(
        bytes.len() == SETTINGS_BYTES,
        "setting.txt must be exactly 256 bytes"
    );
    // Wii setting.txt leaves the zero padding outside the encrypted text.
    let end = bytes
        .iter()
        .rposition(|&byte| byte != 0)
        .map_or(0, |last| last + 1);
    let mut key = 0x73b5_dbfau32;
    for byte in &mut bytes[..end] {
        *byte ^= key as u8;
        key = key.rotate_left(1);
    }
    Ok(bytes)
}

fn decode_settings(bytes: &[u8]) -> Result<ConsoleSettings> {
    let plaintext = crypt_settings(bytes.to_vec())?;
    let end = plaintext
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(plaintext.len());
    ensure!(
        plaintext[end..].iter().all(|byte| *byte == 0),
        "setting.txt has data after its terminator"
    );
    let decoded = plaintext[..end]
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .map(|line| {
            let line = line.strip_suffix(b"\r").unwrap_or(line);
            let equals = line
                .iter()
                .position(|byte| *byte == b'=')
                .context("Malformed setting.txt line")?;
            let key =
                std::str::from_utf8(&line[..equals]).context("setting.txt keys must be ASCII")?;
            let value = line[equals + 1..]
                .iter()
                .map(|byte| char::from(*byte))
                .collect::<String>();
            Ok((key.to_owned(), value))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut settings = BTreeMap::new();
    for (key, value) in decoded {
        ensure!(
            settings.insert(key, value).is_none(),
            "setting.txt contains a duplicate field"
        );
    }
    ensure!(
        settings.len() == 8,
        "setting.txt must contain all eight console fields"
    );
    let fields = ConsoleSettings {
        area: ConsoleSettingValue::try_from(
            settings.remove("AREA").context("setting.txt has no AREA")?,
        )?,
        model: ConsoleSettingValue::try_from(
            settings
                .remove("MODEL")
                .context("setting.txt has no MODEL")?,
        )?,
        dvd: ConsoleSettingValue::try_from(
            settings.remove("DVD").context("setting.txt has no DVD")?,
        )?,
        mpch: ConsoleSettingValue::try_from(
            settings.remove("MPCH").context("setting.txt has no MPCH")?,
        )?,
        code: ConsoleSettingValue::try_from(
            settings.remove("CODE").context("setting.txt has no CODE")?,
        )?,
        serial: ConsoleSettingValue::try_from(
            settings
                .remove("SERNO")
                .context("setting.txt has no SERNO")?,
        )?,
        video: ConsoleSettingValue::try_from(
            settings
                .remove("VIDEO")
                .context("setting.txt has no VIDEO")?,
        )?,
        game: ConsoleSettingValue::try_from(
            settings.remove("GAME").context("setting.txt has no GAME")?,
        )?,
    };
    Ok(fields)
}

const KOREAN_KEY: [u8; 16] = [
    0x63, 0xb8, 0x2b, 0xb4, 0xf4, 0x61, 0x4e, 0x2e, 0x13, 0xf2, 0xfe, 0xfb, 0xba, 0x4c, 0x9b, 0x7e,
];

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use lzma_rust2::{XzOptions, XzWriter};
    use std::fmt::Write as _;
    use std::io::Write;

    fn synthetic_seed() -> Result<String> {
        let boot = vec![0x5a; BOOT_BYTES];
        let mut writer = XzWriter::new(Vec::new(), XzOptions::with_preset(1))?;
        writer.write_all(&boot)?;
        let compressed = writer.finish()?;
        let keys = vec![0; KEYS_BYTES];
        let settings = ConsoleSettings {
            area: ConsoleSettingValue::try_from("EUR".to_owned())?,
            model: ConsoleSettingValue::try_from("RVL-001(EUR)".to_owned())?,
            dvd: ConsoleSettingValue::try_from("0".to_owned())?,
            mpch: ConsoleSettingValue::try_from("0x7FFE".to_owned())?,
            code: ConsoleSettingValue::try_from("LEH".to_owned())?,
            serial: ConsoleSettingValue::try_from("182060878".to_owned())?,
            video: ConsoleSettingValue::try_from("PAL".to_owned())?,
            game: ConsoleSettingValue::try_from("EU".to_owned())?,
        };
        let settings_bytes = settings.encode()?;
        let binary = base64::engine::general_purpose::STANDARD;
        let mut otp = String::new();
        for (name, size) in [
            ("boot1_sha1", 20),
            ("common_key", 16),
            ("console_id", 4),
            ("ng_private_key", 30),
            ("nand_hmac_key", 20),
            ("nand_aes_key", 16),
            ("rng_key", 16),
            ("unknown_78", 8),
        ] {
            writeln!(otp, "    {name}: {}", "00".repeat(size))?;
        }
        let mut seeprom = String::new();
        for (name, size) in [
            ("ms_id", 4),
            ("ca_id", 4),
            ("ng_key_id", 4),
            ("ng_signature", 60),
            ("boot2_counters", 20),
            ("nand_counters", 18),
            ("korean_key", 16),
            ("prng_seed", 4),
        ] {
            writeln!(seeprom, "    {name}: {}", "00".repeat(size))?;
        }
        Ok(format!(
            "source:\n  sha256: {}\nkeys:\n  header: \"\"\n  otp:\n{otp}  seeprom:\n{seeprom}  extra: []\n  _sha256: {}\nbad_blocks: [9]\nboot_blocks:\n  _sha256: {}\n  xz: !!binary {}\nsettings:\n  _sha256: {}\n  fields:\n    AREA: EUR\n    MODEL: RVL-001(EUR)\n    DVD: \"0\"\n    MPCH: 0x7FFE\n    CODE: LEH\n    SERNO: \"182060878\"\n    VIDEO: PAL\n    GAME: EU\nboot1_version: 0\nboot2: []\nbootmii_boot2: unknown\n",
            "00".repeat(32),
            sha256(&keys),
            sha256(&boot),
            binary.encode(compressed),
            sha256(&settings_bytes),
        ))
    }

    #[test]
    fn parses_and_validates_a_synthetic_seed() -> Result<()> {
        let yaml = synthetic_seed()?;
        // A parser diagnostic is safe here because this fixture is generated in the test.
        let _: SeedManifest = serde_saphyr::from_str_with_options(&yaml, seed_yaml_options())?;
        let seed = StockSeed::parse(yaml.as_bytes())?;
        assert_eq!(seed.keys().len(), KEYS_BYTES);
        assert_eq!(seed.boot_blocks().len(), BOOT_BYTES);
        assert_eq!(seed.bad_blocks(), [9]);
        assert_eq!(seed.setting_txt().len(), SETTINGS_BYTES);
        assert_eq!(seed.source_settings().area.as_str(), "EUR");
        Ok(())
    }

    #[test]
    fn rejects_invalid_seed_even_if_it_could_be_retargeted() -> Result<()> {
        let mut yaml = synthetic_seed()?;
        yaml = yaml.replace("GAME: EU", "GAME: US");
        assert!(StockSeed::parse(yaml.as_bytes()).is_err());

        let malformed = b"private_console_secret: [SECRET_SENTINEL: {";
        let error = match StockSeed::parse(malformed) {
            Err(error) => error,
            Ok(_) => panic!("malformed seed unexpectedly parsed"),
        };
        let chain = format!("{error:#}");
        assert_eq!(chain, "Invalid Wii seed YAML");
        assert!(!chain.contains("SECRET_SENTINEL"));
        Ok(())
    }
}
