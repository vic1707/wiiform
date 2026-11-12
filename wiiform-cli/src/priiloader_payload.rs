// SPDX-License-Identifier: GPL-2.0-only
use anyhow::{Context, Result, ensure};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File},
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};
use tempfile::NamedTempFile;
use zip::ZipArchive;

pub const APP_SHA256: &str = "f4508801cc4274bbcb74c07551d591486209c634fbf44f65765685fb19ec0157";
const APP_NAME: &str = "priiloader-0.10.0.app";
const APP_SIZE: usize = 747_968;
const RELEASE_URL: &str =
    "https://github.com/DacoTaco/priiloader/releases/download/0.10.0/Priiloader_v0_10.zip";
const ARCHIVE_NAME: &str = "Priiloader_v0_10.zip";
const ARCHIVE_SHA256: &str = "312881836bc62ae55d496a9caef987d19493f635e26079bc60ecde08a2a31c1d";
const ARCHIVE_LIMIT: usize = 1024 * 1024;
const DEFINITIONS_SHA256: &str = "aea1a0009a327298f0608809c240aaf0ca5f5de3b12ef7ad761901dc4cb74ff4";

const PACKED_OFFSET: usize = 0x8e0;
const PACKED_SIZE: usize = 0x85100;
const INSTALLER_SIZE: usize = 0x11f600;
const EMBEDDED_APP_OFFSET: usize = 0x4ea40;
const END_MARKER: usize = 256;
const MATCH_LENGTHS: usize = 62;
const DISTANCE_BITS: [usize; 6] = [4, 6, 8, 10, 12, 14];
const DISTANCE_BASES: [usize; 6] = [0, 16, 80, 336, 1360, 5456];

struct Huffman {
    left: Vec<usize>,
    right: Vec<usize>,
    parent: Vec<usize>,
    weight: Vec<usize>,
}

impl Huffman {
    const FIRST_LEAF: usize = END_MARKER + 1 + DISTANCE_BITS.len() * MATCH_LENGTHS;
    const NODE_COUNT: usize = 2 * Self::FIRST_LEAF;
    const ROOT: usize = 1;

    fn new() -> Self {
        let mut weight = vec![1; Self::NODE_COUNT];
        weight[..2].fill(0);
        Self {
            left: (0..Self::FIRST_LEAF).map(|node| 2 * node).collect(),
            right: (0..Self::FIRST_LEAF).map(|node| 2 * node + 1).collect(),
            parent: (0..Self::NODE_COUNT).map(|node| node / 2).collect(),
            weight,
        }
    }

    fn sibling(&self, node: usize, parent: usize) -> usize {
        if self.left[parent] == node {
            self.right[parent]
        } else {
            self.left[parent]
        }
    }

    fn replace_child(&mut self, parent: usize, previous: usize, replacement: usize) {
        if self.left[parent] == previous {
            self.left[parent] = replacement;
        } else {
            self.right[parent] = replacement;
        }
        self.parent[replacement] = parent;
    }

    fn update_weights(&mut self, mut node: usize, mut sibling: usize) {
        loop {
            let parent = self.parent[node];
            self.weight[parent] = self.weight[node] + self.weight[sibling];
            if parent == Self::ROOT {
                break;
            }
            node = parent;
            sibling = self.sibling(node, self.parent[node]);
        }
        if self.weight[Self::ROOT] == 2000 {
            for weight in &mut self.weight[Self::ROOT..] {
                *weight /= 2;
            }
        }
    }

    fn read(&mut self, bits: &mut impl Iterator<Item = usize>) -> Result<usize> {
        let mut node = Self::ROOT;
        while node < Self::FIRST_LEAF {
            node = if read_bit(bits)? == 0 {
                self.left[node]
            } else {
                self.right[node]
            };
        }
        let symbol = node - Self::FIRST_LEAF;
        self.weight[node] += 1;
        let mut parent = self.parent[node];
        if parent != Self::ROOT {
            self.update_weights(node, self.sibling(node, parent));
        }
        while parent != Self::ROOT {
            let grandparent = self.parent[parent];
            let uncle = self.sibling(parent, grandparent);
            if self.weight[node] > self.weight[uncle] {
                let sibling = self.sibling(node, parent);
                self.replace_child(grandparent, uncle, node);
                self.replace_child(parent, node, uncle);
                node = uncle;
                self.update_weights(node, sibling);
            }
            node = self.parent[node];
            parent = self.parent[node];
        }
        Ok(symbol)
    }
}

fn read_bit(bits: &mut impl Iterator<Item = usize>) -> Result<usize> {
    bits.next()
        .context("truncated compressed Priiloader installer")
}

fn expand_installer(packed: &[u8]) -> Result<Vec<u8>> {
    let mut bits = packed.iter().flat_map(|byte| {
        (0..8)
            .rev()
            .map(move |shift| usize::from((byte >> shift) & 1))
    });
    let mut tree = Huffman::new();
    let mut output = Vec::with_capacity(INSTALLER_SIZE);
    loop {
        let symbol = tree.read(&mut bits)?;
        match symbol.cmp(&END_MARKER) {
            std::cmp::Ordering::Less => output.push(symbol as u8),
            std::cmp::Ordering::Equal => return Ok(output),
            std::cmp::Ordering::Greater => {
                let code = symbol - END_MARKER - 1;
                let group = code / MATCH_LENGTHS;
                let length = code % MATCH_LENGTHS + 3;
                let mut extra = 0;
                for shift in 0..DISTANCE_BITS[group] {
                    extra |= read_bit(&mut bits)? << shift;
                }
                let distance = DISTANCE_BASES[group] + length + extra;
                ensure!(distance <= output.len(), "invalid Priiloader backreference");
                let start = output.len() - distance;
                output.extend_from_within(start..start + length);
            }
        }
        ensure!(
            output.len() <= INSTALLER_SIZE,
            "expanded Priiloader installer exceeds its pinned size"
        );
    }
}

fn verify(data: &[u8], expected: &str, limit: usize, label: &str) -> Result<()> {
    ensure!(
        data.len() <= limit && format!("{:x}", Sha256::digest(data)) == expected,
        "{label} does not match its pinned SHA-256"
    );
    Ok(())
}

fn bounded_read(path: &Path, limit: usize) -> Result<Vec<u8>> {
    ensure!(
        fs::symlink_metadata(path)?.is_file(),
        "expected a regular file at {}",
        path.display()
    );
    read_limit(File::open(path)?, limit)
}

fn read_limit(reader: impl Read, limit: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take((limit + 1) as u64).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= limit,
        "Priiloader input exceeds its size limit"
    );
    Ok(bytes)
}

fn archive_member(archive: &[u8], name: &str, limit: usize) -> Result<Vec<u8>> {
    let mut zip = ZipArchive::new(Cursor::new(archive))?;
    read_limit(zip.by_name(name)?, limit)
}

fn extract_app(archive: &[u8]) -> Result<Vec<u8>> {
    let dol = archive_member(
        archive,
        "apps/priiloader/boot.dol",
        PACKED_OFFSET + PACKED_SIZE,
    )?;
    ensure!(
        dol.len() == PACKED_OFFSET + PACKED_SIZE,
        "unexpected Priiloader installer size"
    );
    let installer = expand_installer(&dol[PACKED_OFFSET..])?;
    ensure!(
        installer.len() == INSTALLER_SIZE,
        "unexpected expanded Priiloader installer size"
    );
    let app = &installer[EMBEDDED_APP_OFFSET..EMBEDDED_APP_OFFSET + APP_SIZE];
    verify(app, APP_SHA256, APP_SIZE, "Priiloader app")?;
    Ok(app.to_vec())
}

fn publish_cache(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .context("Priiloader cache path has no parent")?;
    let mut staged = NamedTempFile::new_in(parent)?;
    staged.write_all(data)?;
    staged.as_file().sync_all()?;
    match staged.persist_noclobber(path) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn release(cache: &Path, offline: bool) -> Result<Vec<u8>> {
    let path = cache.join(ARCHIVE_NAME);
    if !path.exists() && !path.is_symlink() {
        ensure!(
            !offline,
            "offline Priiloader cache miss at {}",
            path.display()
        );
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(30)))
            .max_redirects(5)
            .https_only(true)
            .build()
            .into();
        let response = agent
            .get(RELEASE_URL)
            .call()
            .context("download pinned Priiloader release")?;
        let bytes = read_limit(response.into_body().into_reader(), ARCHIVE_LIMIT)?;
        verify(&bytes, ARCHIVE_SHA256, ARCHIVE_LIMIT, "Priiloader release")?;
        publish_cache(&path, &bytes)?;
    }
    let bytes = bounded_read(&path, ARCHIVE_LIMIT)?;
    verify(&bytes, ARCHIVE_SHA256, ARCHIVE_LIMIT, "Priiloader release")?;
    Ok(bytes)
}

pub struct Assets {
    pub app: PathBuf,
    pub definitions: Option<PathBuf>,
}

pub fn prepare(
    cache: &Path,
    workspace: &Path,
    offline: bool,
    explicit_app: Option<&Path>,
    want_definitions: bool,
) -> Result<Assets> {
    let cache = cache.join("priiloader");
    ensure!(
        !cache.is_symlink(),
        "Priiloader cache must be a real directory"
    );
    let app = explicit_app
        .map(Path::to_owned)
        .unwrap_or_else(|| cache.join(APP_NAME));
    let missing_app = explicit_app.is_none() && !app.exists() && !app.is_symlink();
    let archive = if missing_app || want_definitions {
        if !offline {
            fs::create_dir_all(&cache)?;
        }
        Some(release(&cache, offline)?)
    } else {
        None
    };
    if missing_app {
        let archive = archive
            .as_deref()
            .context("missing pinned Priiloader release")?;
        publish_cache(&app, &extract_app(archive)?)?;
    }
    verify(
        &bounded_read(&app, APP_SIZE)?,
        APP_SHA256,
        APP_SIZE,
        "Priiloader app",
    )?;
    let definitions = if want_definitions {
        let archive = archive
            .as_deref()
            .context("missing pinned Priiloader release")?;
        let data = archive_member(archive, "apps/priiloader/hacks_hash.ini", 64 * 1024)?;
        verify(
            &data,
            DEFINITIONS_SHA256,
            64 * 1024,
            "Priiloader hack definitions",
        )?;
        let path = workspace.join("hacks_hash.ini");
        let mut file = File::options().write(true).create_new(true).open(&path)?;
        file.write_all(&data)?;
        Some(path)
    } else {
        None
    };
    Ok(Assets { app, definitions })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_packed_stream_is_rejected() {
        assert!(expand_installer(&[]).is_err());
    }

    #[test]
    fn pinned_release_when_supplied() -> Result<()> {
        let Some(path) = std::env::var_os("WIIFORM_TEST_PRIILOADER_RELEASE") else {
            return Ok(());
        };
        let archive = bounded_read(Path::new(&path), ARCHIVE_LIMIT)?;
        verify(
            &archive,
            ARCHIVE_SHA256,
            ARCHIVE_LIMIT,
            "Priiloader release",
        )?;
        let app = extract_app(&archive)?;
        ensure!(app.len() == APP_SIZE, "incorrect extracted app length");
        let definitions = archive_member(&archive, "apps/priiloader/hacks_hash.ini", 64 * 1024)?;
        verify(
            &definitions,
            DEFINITIONS_SHA256,
            64 * 1024,
            "Priiloader definitions",
        )
    }
}
