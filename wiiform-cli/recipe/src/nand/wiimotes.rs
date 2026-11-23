// SPDX-License-Identifier: GPL-2.0-only
//! The `wiimotes` key: the Wiimotes the console has registered, kept from the seed.
//! They are written into SYSCONF with the system settings, so `setup` is required.
use anyhow::{Result, ensure};
use schemars::JsonSchema;
use serde::Deserialize;

/// Length of the BT.DINF value in SYSCONF.
pub const REGISTRATION_BYTES: usize = 0x461;
/// One registration: a Bluetooth address, then a 64-byte name, NUL-padded.
const SLOT_BYTES: usize = 6 + 64;
const NAME_MAX: usize = 63;
/// How many registrations the value holds: a count byte, then the slots.
pub const REGISTRATIONS_MAX: usize = (REGISTRATION_BYTES - 1) / SLOT_BYTES;

/// The registered Wiimotes, copied from the seed's console.
#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Wiimotes {
    /// Copy the registrations the seed holds.
    Keep,
}

/// A Wiimote the console has registered.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Registration {
    pub address: [u8; 6],
    pub name: Vec<u8>,
}

/// The BT.DINF value for `registrations`, with nothing else set.
pub fn encode_registrations(registrations: &[Registration]) -> Result<Vec<u8>> {
    ensure!(
        registrations.len() <= REGISTRATIONS_MAX,
        "at most {REGISTRATIONS_MAX} Wiimotes can be registered"
    );
    let mut value = vec![0u8; REGISTRATION_BYTES];
    value[0] = registrations.len() as u8;
    for (index, registration) in registrations.iter().enumerate() {
        ensure!(
            registration.name.len() <= NAME_MAX && !registration.name.contains(&0),
            "a Wiimote name is at most 63 bytes with no NUL"
        );
        let at = 1 + index * SLOT_BYTES;
        value[at..at + 6].copy_from_slice(&registration.address);
        value[at + 6..at + 6 + registration.name.len()].copy_from_slice(&registration.name);
    }
    Ok(value)
}

/// The registrations a BT.DINF value holds, when its slots have this layout. The
/// caller checks that `encode_registrations` gives back the same value.
pub fn decode_registrations(value: &[u8]) -> Option<Vec<Registration>> {
    if value.len() != REGISTRATION_BYTES {
        return None;
    }
    let count = usize::from(value[0]);
    if count > REGISTRATIONS_MAX {
        return None;
    }
    (0..count)
        .map(|index| {
            let at = 1 + index * SLOT_BYTES;
            let address: [u8; 6] = value[at..at + 6].try_into().ok()?;
            let name = &value[at + 6..at + SLOT_BYTES];
            let end = name.iter().position(|&byte| byte == 0)?;
            Some(Registration {
                address,
                name: name[..end].to_vec(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wiimote(last: u8, name: &str) -> Registration {
        Registration {
            address: [0xa4, 0x5c, 0x27, 0x09, 0xf9, last],
            name: name.as_bytes().to_vec(),
        }
    }

    #[test]
    fn registrations_round_trip_through_the_value() -> Result<()> {
        let list = vec![
            wiimote(0x8a, "Nintendo RVL-CNT-01"),
            wiimote(0x8b, "Second"),
        ];
        let value = encode_registrations(&list)?;
        assert_eq!(value.len(), REGISTRATION_BYTES);
        assert_eq!(value[0], 2);
        assert_eq!(&value[1..7], &[0xa4, 0x5c, 0x27, 0x09, 0xf9, 0x8a]);
        assert_eq!(&value[7..26], b"Nintendo RVL-CNT-01");
        assert_eq!(decode_registrations(&value), Some(list));
        Ok(())
    }

    #[test]
    fn an_unpaired_console_has_a_zero_count() -> Result<()> {
        let value = encode_registrations(&[])?;
        assert!(value.iter().all(|&byte| byte == 0));
        assert_eq!(decode_registrations(&value), Some(Vec::new()));
        Ok(())
    }

    #[test]
    fn values_the_layout_cannot_describe_are_not_decoded() {
        let mut too_many = vec![0u8; REGISTRATION_BYTES];
        too_many[0] = (REGISTRATIONS_MAX + 1) as u8;
        assert!(decode_registrations(&too_many).is_none());
        // A 64-byte name field with no NUL in it.
        let mut unnamed = encode_registrations(&[wiimote(1, "x")]).unwrap_or_default();
        unnamed[7..7 + 64].fill(b'n');
        assert!(decode_registrations(&unnamed).is_none());
        assert!(decode_registrations(&[0u8; 10]).is_none());
    }

    #[test]
    fn names_are_at_most_63_bytes() {
        assert!(encode_registrations(&[wiimote(1, &"n".repeat(63))]).is_ok());
        assert!(encode_registrations(&[wiimote(1, &"n".repeat(64))]).is_err());
    }
}
