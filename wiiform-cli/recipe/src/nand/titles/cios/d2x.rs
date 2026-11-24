// SPDX-License-Identifier: GPL-2.0-only
//! Building a d2x cIOS from its release: the modules and ciosmaps.xml.
//!
//! The map's format follows d2x's installer; how it is applied follows
//! WiiPy's `ciosbuild` (MIT, https://github.com/NinjaCheetah/WiiPy).
use anyhow::{Context, Result, bail, ensure};

use crate::archive;

/// One change to a content of the base IOS.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContentPatch {
    pub cid: u32,
    pub offset: u32,
    pub original: Vec<u8>,
    pub replacement: Vec<u8>,
}

/// A d2x module added to the base IOS.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Module {
    pub cid: u32,
    /// The TMD position it takes (the content there moves to the end), or None
    /// to append it.
    pub position: Option<u16>,
    pub bytes: Vec<u8>,
}

/// What it takes to build one cIOS: the NUS base and the changes to it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Build {
    /// The base IOS version the map requires, downloaded from NUS.
    pub base_version: u16,
    pub patches: Vec<ContentPatch>,
    pub modules: Vec<Module>,
}

fn number(text: &str) -> Result<u32> {
    let hex = text
        .trim()
        .strip_prefix("0x")
        .or_else(|| text.trim().strip_prefix("0X"));
    Ok(match hex {
        Some(hex) => u32::from_str_radix(hex, 16)?,
        None => text.trim().parse()?,
    })
}

fn byte_list(text: &str) -> Result<Vec<u8>> {
    text.split(',')
        .map(|byte| Ok(u8::try_from(number(byte)?)?))
        .collect()
}

fn attribute<'a>(node: roxmltree::Node<'a, '_>, name: &str) -> Result<&'a str> {
    node.attribute(name)
        .with_context(|| format!("<{}> lacks {name}", node.tag_name().name()))
}

impl Build {
    /// A command-line d2x release, a URL or the path of an existing file, with
    /// its SHA-256: the build for `base`.
    pub fn from_release(
        input: std::path::PathBuf,
        sha256: crate::Sha256,
        base: u8,
    ) -> Result<Self> {
        let release = crate::source::Source::from_command_line(input)?
            .read_checked(Some(sha256), crate::source::MEMORY_BOUNDS)?;
        build(&release, base)
    }
}

/// Reads a d2x release archive and returns the build for `base`.
pub fn build(release: &[u8], base: u8) -> Result<Build> {
    let files = archive::files(release)?;
    let maps: Vec<&(String, Vec<u8>)> = (files.iter())
        .filter(|(name, _)| name.rsplit('/').next() == Some("ciosmaps.xml"))
        .collect();
    let [(map_path, map)] = maps.as_slice() else {
        bail!(
            "a d2x release holds one ciosmaps.xml, this one {}",
            maps.len()
        );
    };
    let folder = map_path
        .rsplit_once('/')
        .map_or(String::new(), |(parent, _)| format!("{parent}/"));
    let text = std::str::from_utf8(map).context("ciosmaps.xml is not UTF-8")?;
    let document = roxmltree::Document::parse(text).context("invalid ciosmaps.xml")?;

    let groups: Vec<_> = document
        .root_element()
        .children()
        .filter(|node| node.has_tag_name("ciosgroup"))
        .collect();
    let [group] = groups.as_slice() else {
        bail!(
            "ciosmaps.xml lists {} cIOS groups; wiiform supports one",
            groups.len()
        );
    };
    let bases: Vec<_> = group
        .children()
        .filter(|node| node.has_tag_name("base"))
        .collect();
    let selected = (bases.iter()).find(|node| {
        node.attribute("ios").and_then(|ios| number(ios).ok()) == Some(u32::from(base))
    });
    let Some(selected) = selected else {
        let supported: Vec<&str> = bases
            .iter()
            .filter_map(|node| node.attribute("ios"))
            .collect();
        bail!(
            "this d2x release has no base IOS{base}; it supports {}",
            supported.join(", ")
        );
    };

    let modules_folder = format!("{folder}{}/", attribute(*group, "name")?);
    let mut build = Build {
        base_version: u16::try_from(number(attribute(*selected, "version")?)?)?,
        patches: Vec::new(),
        modules: Vec::new(),
    };
    for content in selected
        .children()
        .filter(|node| node.has_tag_name("content"))
    {
        let cid = number(attribute(content, "id")?)?;
        for patch in content.children().filter(|node| node.has_tag_name("patch")) {
            let original = byte_list(attribute(patch, "originalbytes")?)?;
            let replacement = byte_list(attribute(patch, "newbytes")?)?;
            let size = usize::try_from(number(attribute(patch, "size")?)?)?;
            ensure!(
                original.len() == size && replacement.len() == size,
                "a patch of content {cid:#x} does not match its size"
            );
            build.patches.push(ContentPatch {
                cid,
                offset: number(attribute(patch, "offset")?)?,
                original,
                replacement,
            });
        }
        if let Some(module) = content.attribute("module") {
            let position = attribute(content, "tmdmoduleid")?.trim();
            let position = match position {
                "-1" => None,
                other => Some(u16::try_from(number(other)?)?),
            };
            let path = format!("{modules_folder}{module}.app");
            let (_, bytes) = (files.iter())
                .find(|(name, _)| *name == path)
                .with_context(|| format!("the d2x release lacks {path}"))?;
            build.modules.push(Module {
                cid,
                position,
                bytes: bytes.clone(),
            });
        }
    }
    Ok(build)
}
