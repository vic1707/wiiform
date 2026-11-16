// SPDX-License-Identifier: GPL-2.0-only
/// Region code a title declares in its TMD.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    Japan,
    Usa,
    Pal,
    /// Runs on every console.
    Free,
    Korea,
}

impl Region {
    pub const fn from_code(code: u16) -> Option<Self> {
        Some(match code {
            0 => Self::Japan,
            1 => Self::Usa,
            2 => Self::Pal,
            3 => Self::Free,
            4 => Self::Korea,
            _ => return None,
        })
    }
}
