// SPDX-License-Identifier: GPL-2.0-only
//! BootMii's settings file, `/bootmii/bootmii.ini`. BootMii never writes it; the
//! recipe does.
//!
//! The keys come from a wiki page, not from BootMii's source, which the wiki
//! does not link: check them on a console before relying on one.
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

use super::CardPath;

/// The video mode BootMii sets.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum BootmiiVideo {
    Ntsc,
    Pal50,
    Pal60,
    Progressive,
}

impl BootmiiVideo {
    fn value(self) -> &'static str {
        match self {
            Self::Ntsc => "NTSC",
            Self::Pal50 => "PAL50",
            Self::Pal60 => "PAL60",
            Self::Progressive => "PROGRESSIVE",
        }
    }
}

/// What BootMii starts once its boot delay runs out.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
enum BootmiiAutoboot {
    /// The Homebrew Channel, through its "Launch BootMii" entry.
    Hbc,
    /// The System Menu.
    Sysmenu,
}

impl BootmiiAutoboot {
    fn value(self) -> &'static str {
        match self {
            Self::Hbc => "HBC",
            Self::Sysmenu => "SYSMENU",
        }
    }
}

/// BootMii's settings, written to `/bootmii/bootmii.ini`, one `KEY=value` line per
/// key given. A key left out keeps BootMii's own default.
#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(super) struct BootmiiIni {
    /// The video mode BootMii sets.
    video: Option<BootmiiVideo>,
    /// What BootMii starts after the boot delay.
    autoboot: Option<BootmiiAutoboot>,
    /// BootMii's boot delay. Its unit is not documented.
    boot_delay: Option<u8>,
}

impl BootmiiIni {
    /// The file's path and contents. Refuses settings that set no key.
    pub(super) fn file(&self) -> Result<(CardPath, Vec<u8>)> {
        let mut lines = Vec::new();
        if let Some(video) = self.video {
            lines.push(format!("VIDEO={}", video.value()));
        }
        if let Some(autoboot) = self.autoboot {
            lines.push(format!("AUTOBOOT={}", autoboot.value()));
        }
        if let Some(delay) = self.boot_delay {
            lines.push(format!("BOOTDELAY={delay}"));
        }
        ensure!(!lines.is_empty(), "bootmii sets no key");
        let mut text = lines.join("\n");
        text.push('\n');
        let path = CardPath::try_from(String::from("/bootmii/bootmii.ini"))?;
        Ok((path, text.into_bytes()))
    }
}
