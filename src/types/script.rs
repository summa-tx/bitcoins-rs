use std::ops::{Index, IndexMut};

use crate::types::primitives::{ConcretePrefixVec, PrefixVec, TxResult};

/// A WitnessStackItem is a marked ConcretePrefixVec<u8> intended for use in witnesses. Each
/// Witness is a `PrefixVec<WitnessStackItem>`. The Transactions `witnesses` is a non-prefixed
/// `Vec<Witness>.`
///
/// `WitnessStackItem::null()` and `WitnessStackItem::default()` return the empty byte vector with
/// a 0 prefix, which represents numerical 0, or null bytestring.
///
/// TODO: Witness stack items do not permit non-minimal VarInt prefixes. Return an error if the
/// user tries to pass one in to `set_prefix_len`.
///
/// TODO: change set_items into push_item.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct WitnessStackItem(pub ConcretePrefixVec<u8>);

impl PrefixVec for WitnessStackItem {
    type Item = u8;

    fn null() -> Self {
        Self(Default::default())
    }

    fn set_items(&mut self, v: Vec<Self::Item>) -> TxResult<()> {
        self.0.set_items(v)
    }

    fn set_prefix_len(&mut self, prefix_len: u8) -> TxResult<()> {
        self.0.set_prefix_len(prefix_len)
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn len_prefix(&self) -> u8 {
        self.0.len_prefix()
    }

    fn items(&self) -> &[Self::Item] {
        self.0.items()
    }
}
impl<T> From<T> for WitnessStackItem
where
    T: Into<ConcretePrefixVec<u8>>
{
    fn from(v: T) -> Self {
        Self(v.into())
    }
}

impl Index<usize> for WitnessStackItem {
    type Output = u8;

    fn index(&self, index: usize) -> &u8 {
        &self.0[index]
    }
}

impl IndexMut<usize> for WitnessStackItem {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

/// A Script is a marked ConcretePrefixVec<u8> for use in the script_sig, and script_pubkey
/// fields.
///
/// `Script::null()` and `Script::default()` return the empty byte vector with a 0 prefix, which
/// represents numerical 0, or null bytestring.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct Script(pub ConcretePrefixVec<u8>);

#[derive(PartialEq, Eq, Clone, Debug)]
pub enum ScriptType {
    PKH,
    SH,
    WPKH,
    WSH,
    NonStandard
}

impl Script {
    pub fn determine_type(&self) -> ScriptType {
        let items = self.0.items();
        match self.0.len() {
            0x19 => {
                // PKH;
                if items[0..=2] == [0x76, 0xa9, 0x14] && items[17..=18] == [0x88, 0xac] {
                    ScriptType::PKH
                } else {
                    ScriptType::NonStandard
                }
            },
            0x17 => {
                // SH
                if items[0..=2] == [0xa9, 0x14] && items[17..=18] == [0x87] {
                    ScriptType::SH
                } else {
                    ScriptType::NonStandard
                }
            },
            0x16 => {
                // WPKH
                if items[0..=1] == [0x00, 0x14] {
                    ScriptType::WPKH
                } else {
                    ScriptType::NonStandard
                }
            },
            0x22 => {
                if items[0..=1] == [0x00, 0x20] {
                    ScriptType::WSH
                } else {
                    ScriptType::NonStandard
                }
            },
            _ => ScriptType::NonStandard
        }
    }
}

impl PrefixVec for Script {
    type Item = u8;

    /// Return a null (empty) witness stack item. This item represents numerical 0, or the null
    /// string.
    fn null() -> Self {
        Self(Default::default())
    }

    fn set_items(&mut self, v: Vec<Self::Item>) -> TxResult<()> {
        self.0.set_items(v)
    }

    fn set_prefix_len(&mut self, prefix_len: u8) -> TxResult<()> {
        self.0.set_prefix_len(prefix_len)
    }

    fn len(&self) -> usize {
        self.0.len()
    }

    fn len_prefix(&self) -> u8 {
        self.0.len_prefix()
    }

    fn items(&self) -> &[Self::Item] {
        self.0.items()
    }
}

impl<T> From<T> for Script
where
    T: Into<ConcretePrefixVec<u8>>
{
    fn from(v: T) -> Self {
        Self(v.into())
    }
}

impl Index<usize> for Script {
    type Output = u8;

    fn index(&self, index: usize) -> &u8 {
        &self.0[index]
    }
}

impl IndexMut<usize> for Script {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.0[index]
    }
}

pub type Witness = ConcretePrefixVec<WitnessStackItem>;

#[cfg(test)]
mod test{
    use super::*;
    use crate::types::primitives::{Ser, PrefixVec};

    #[test]
    fn it_serializes_and_derializes_scripts() {
        let cases = [
        (
            Script::new(hex::decode("0014758ce550380d964051086798d6546bebdca27a73".to_owned()).unwrap()),
            "160014758ce550380d964051086798d6546bebdca27a73",
            22
        ),
        (
            Script::new(vec![]),
            "00",
            0
        ),
        (
            Script::null(),
            "00",
            0
        ),
        (
            Script::new_non_minimal(vec![], 9).unwrap(),
            "ff0000000000000000",
            0
        ),
        ];
        for case in cases.iter() {
            let prevout_script = Script::deserialize_hex(case.1.to_owned()).unwrap();
            assert_eq!(case.0.serialize_hex().unwrap(), case.1);
            assert_eq!(case.0.len(), case.2);
            assert_eq!(case.0.is_empty(), case.2 == 0);

            assert_eq!(prevout_script, case.0);
            assert_eq!(prevout_script.serialize_hex().unwrap(), case.1);
            assert_eq!(prevout_script.len(), case.2);
            assert_eq!(prevout_script.is_empty(), case.2 == 0);

        }
    }

    #[test]
    fn it_serializes_and_derializes_witness_stack_items() {
        let cases = [
        (
            WitnessStackItem::new(hex::decode("0014758ce550380d964051086798d6546bebdca27a73".to_owned()).unwrap()),
            "160014758ce550380d964051086798d6546bebdca27a73",
            22
        ),
        (
            WitnessStackItem::new(vec![]),
            "00",
            0
        ),
        (
            WitnessStackItem::null(),
            "00",
            0
        ),
        ];
        for case in cases.iter() {
            let prevout_script = WitnessStackItem::deserialize_hex(case.1.to_owned()).unwrap();
            assert_eq!(case.0.serialize_hex().unwrap(), case.1);
            assert_eq!(case.0.len(), case.2);
            assert_eq!(case.0.is_empty(), case.2 == 0);

            assert_eq!(prevout_script, case.0);
            assert_eq!(prevout_script.serialize_hex().unwrap(), case.1);
            assert_eq!(prevout_script.len(), case.2);
            assert_eq!(prevout_script.is_empty(), case.2 == 0);

        }
    }
}
