// SPDX-License-Identifier: GPL-2.0-only
use crate::TitleId;

/// One IOS RVLoader's installer (v2.1) writes, derived from a stock IOS.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Ios {
    pub source: TitleId,
    pub version: u16,
    pub slot: u8,
    /// Kernel patches: fakesigning, ES_Identify, ISFS permissions, downgrades.
    pub es_patches: bool,
    /// The stock Wi-Fi modules give way to RVLoader's driver.
    pub without_wifi: bool,
}

const IOS36: (TitleId, u16) = (TitleId::known(0x0000_0001_0000_0024), 3608);
const IOS58: (TitleId, u16) = (TitleId::known(0x0000_0001_0000_003a), 6176);
const IOS80: (TitleId, u16) = (TitleId::known(0x0000_0001_0000_0050), 6944);

impl Ios {
    const fn new(
        (source, version): (TitleId, u16),
        slot: u8,
        es_patches: bool,
        without_wifi: bool,
    ) -> Self {
        Self {
            source,
            version,
            slot,
            es_patches,
            without_wifi,
        }
    }

    /// installer/source/main.cpp: three copies without Wi-Fi, then the stock
    /// slots themselves, without Wi-Fi only if the user asked for it.
    pub(super) const fn installed(patch_out_wifi: bool) -> [Self; 6] {
        [
            Self::new(IOS36, 236, true, true),
            Self::new(IOS58, 240, false, true),
            Self::new(IOS80, 241, false, true),
            Self::new(IOS36, 36, false, patch_out_wifi),
            Self::new(IOS58, 58, false, patch_out_wifi),
            Self::new(IOS80, 80, false, patch_out_wifi),
        ]
    }
}
