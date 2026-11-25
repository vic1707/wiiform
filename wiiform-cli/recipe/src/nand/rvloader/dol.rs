// SPDX-License-Identifier: GPL-2.0-only
//! The DOLs an executable carries as data, such as the bootloader inside
//! RVLoader's installer.
use std::ops::Range;

const HEADER: usize = 0x100;
const SECTIONS: usize = 18;
const TEXT_SECTIONS: usize = 7;
/// MEM1, where a DOL's sections and its entry point live.
const MEM1: Range<u32> = 0x8000_0000..0x8180_0000;

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().expect("four bytes"))
}

/// The length of the DOL whose header starts `bytes`, if the header is one: each
/// section lies after the header and inside `bytes`, loads into MEM1, and the
/// entry point is in a text section.
fn length(bytes: &[u8]) -> Option<usize> {
    if bytes.len() < HEADER {
        return None;
    }
    let field = |table: usize, section: usize| word(bytes, (table * SECTIONS + section) * 4);
    let mut end = HEADER;
    let mut entry_in_text = false;
    let entry = word(bytes, 0xe0);
    for section in 0..SECTIONS {
        let (offset, address, size) = (field(0, section), field(1, section), field(2, section));
        if size == 0 {
            continue;
        }
        let (start, size_bytes) = (offset as usize, size as usize);
        let fits = start >= HEADER && start % 4 == 0 && start + size_bytes <= bytes.len();
        if !fits || address % 4 != 0 || !MEM1.contains(&address) {
            return None;
        }
        end = end.max(start + size_bytes);
        if section < TEXT_SECTIONS && (address..address.saturating_add(size)).contains(&entry) {
            entry_in_text = true;
        }
    }
    let (bss_address, bss_size) = (word(bytes, 0xd8), word(bytes, 0xdc));
    (entry_in_text && (bss_size == 0 || MEM1.contains(&bss_address))).then_some(end)
}

/// The DOLs inside `executable`, itself a DOL, at word-aligned offsets.
pub(super) fn embedded(executable: &[u8]) -> impl Iterator<Item = &[u8]> {
    (4..executable.len().saturating_sub(HEADER))
        .step_by(4)
        .filter_map(|at| length(&executable[at..]).map(|length| &executable[at..at + length]))
}

/// A DOL of `text` loaded and entered at 0x80004000, for tests.
#[cfg(test)]
pub(crate) fn sample(text: &[u8]) -> Vec<u8> {
    let mut dol = vec![0; HEADER];
    let size = u32::try_from(text.len().next_multiple_of(4)).expect("a small text");
    dol[0..4].copy_from_slice(&0x100_u32.to_be_bytes());
    dol[0x48..0x4c].copy_from_slice(&0x8000_4000_u32.to_be_bytes());
    dol[0x90..0x94].copy_from_slice(&size.to_be_bytes());
    dol[0xe0..0xe4].copy_from_slice(&0x8000_4000_u32.to_be_bytes());
    dol.extend_from_slice(text);
    dol.resize(HEADER + size as usize, 0);
    dol
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_dols_carried_as_data() {
        let inner = sample(b"inner text");
        let mut outer = sample(&[0; 64]);
        outer.extend_from_slice(&[0xaa; 12]);
        outer.extend_from_slice(&inner);
        outer.extend_from_slice(&[0xbb; 8]);
        assert_eq!(embedded(&outer).collect::<Vec<_>>(), [&inner[..]]);

        let mut entry_outside = inner.clone();
        entry_outside[0xe0..0xe4].copy_from_slice(&0x8000_0000_u32.to_be_bytes());
        assert_eq!(length(&entry_outside), None);
        assert_eq!(length(&inner[..inner.len() - 4]), None);
        assert_eq!(length(&inner), Some(inner.len()));
    }
}
