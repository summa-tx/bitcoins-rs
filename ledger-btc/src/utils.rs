use coins_core::{
    ser::{self, ByteFormat},
};
use coins_bip32::{
    curve::model::{PointDeserialize, SigSerialize},
    derived::DerivedXPub,
    keys::Pubkey,
    primitives::ChainCode,
    path::{DerivationPath},
};
use bitcoins::types::{BitcoinTxIn, TxOut, UTXO, ScriptType, SpendScript};
use coins_ledger::{
    common::{APDUAnswer, APDUCommand, APDUData},
};

use crate::LedgerBTCError;


#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
#[allow(non_camel_case_types)]
pub(crate) enum Commands {
    GET_WALLET_PUBLIC_KEY = 0x40,
    UNTRUSTED_HASH_TX_INPUT_START = 0x44,
    UNTRUSTED_HASH_SIGN = 0x48,
    UNTRUSTED_HASH_TX_INPUT_FINALIZE_FULL = 0x4a,
}

pub(crate) struct InternalKeyInfo {
    pub(crate) pubkey: Pubkey,
    pub(crate) path: DerivationPath,
    pub(crate) chain_code: ChainCode,
}

pub(crate) fn parse_pubkey_response(deriv: &DerivationPath, data: &[u8]) -> InternalKeyInfo {
    let mut chain_code = [0u8; 32];
    chain_code.copy_from_slice(&data[data.len() - 32..]);

    let mut pk = [0u8; 65];
    pk.copy_from_slice(&data[1..66]);
    InternalKeyInfo {
        pubkey: coins_bip32::keys::Pubkey {
            key: PointDeserialize::from_pubkey_array_uncompressed(pk).unwrap(),
            backend: Some(coins_bip32::Secp256k1::static_ref()),
        },
        path: deriv.clone(),
        chain_code: chain_code.into(),
    }
}

// Convert a derivation path to its apdu data format
pub(crate) fn derivation_path_to_apdu_data(deriv: &DerivationPath) -> APDUData {
    let mut buf = vec![];
    buf.push(deriv.len() as u8);
    for idx in deriv.iter() {
        buf.extend(&idx.to_be_bytes());
    }
    APDUData::from(buf)
}

pub(crate) fn untrusted_hash_tx_input_start(chunk: &[u8], first: bool) -> APDUCommand {
    APDUCommand {
        ins: Commands::UNTRUSTED_HASH_TX_INPUT_START as u8,
        p1: if first { 0x00 } else { 0x80 },
        p2: 0x02,
        data: APDUData::from(chunk),
        response_len: Some(64),
    }
}

pub(crate) fn untrusted_hash_tx_input_finalize(chunk: &[u8], last: bool) -> APDUCommand {
    APDUCommand {
        ins: Commands::UNTRUSTED_HASH_TX_INPUT_FINALIZE_FULL as u8,
        p1: if last { 0x80 } else { 0x00 },
        p2: 0x00,
        data: APDUData::from(chunk),
        response_len: Some(64),
    }
}

pub(crate) fn untrusted_hash_sign(chunk: &[u8]) -> APDUCommand {
    APDUCommand {
        ins: Commands::UNTRUSTED_HASH_SIGN as u8,
        p1: 0x00,
        p2: 0x00,
        data: APDUData::from(chunk),
        response_len: Some(64),
    }
}

pub(crate) fn packetize_version_and_vin_length(version: u32, vin_len: u64) -> APDUCommand {
    let mut chunk = vec![];
    chunk.extend(&version.to_le_bytes());
    ser::write_compact_int(&mut chunk, vin_len).unwrap();
    untrusted_hash_tx_input_start(&chunk, true)
}

pub(crate) fn packetize_input(utxo: &UTXO, txin: &BitcoinTxIn) -> Vec<APDUCommand> {
    let mut buf = vec![0x02];
    txin.outpoint.write_to(&mut buf).unwrap();
    buf.extend(&utxo.value.to_le_bytes());
    buf.push(0x00);

    let first = untrusted_hash_tx_input_start(&buf, false);
    let second = untrusted_hash_tx_input_start(&txin.sequence.to_le_bytes(), false);

    vec![first, second]
}

pub(crate) fn packetize_input_for_signing(utxo: &UTXO, txin: &BitcoinTxIn) -> Vec<APDUCommand> {
    let mut buf = vec![0x02];
    txin.outpoint.write_to(&mut buf).unwrap();
    buf.extend(&utxo.value.to_le_bytes());
    buf.extend(utxo.signing_script().unwrap()); // should have been preflighted by `should_sign`

    buf.chunks(50)
        .map(|d| untrusted_hash_tx_input_start(&d, false))
        .collect()
}

pub(crate) fn packetize_vout(outputs: &[TxOut]) -> Vec<APDUCommand> {
    let mut buf = vec![];
    ser::write_compact_int(&mut buf, outputs.len() as u64).unwrap();
    for output in outputs.iter() {
        output.write_to(&mut buf).unwrap();
    }

    let mut packets = vec![];
    // The last chunk will
    let mut chunks = buf.chunks(50).peekable();
    while let Some(chunk) = chunks.next() {
        packets.push(untrusted_hash_tx_input_finalize(
            &chunk,
            chunks.peek().is_none(),
        ))
    }
    packets
}

pub(crate) fn transaction_final_packet(lock_time: u32, path: &DerivationPath) -> APDUCommand {
    let mut buf = vec![];
    buf.extend(derivation_path_to_apdu_data(&path).data());
    buf.push(0x00); // deprecated
    buf.extend(&lock_time.to_le_bytes());
    buf.push(0x01); // SIGHASH_ALL
    untrusted_hash_sign(&buf)
}

// This is ugly.
pub(crate) fn modify_tx_start_packet(command: &APDUCommand) -> APDUCommand {
    let mut c = command.clone();

    let mut new_data = c.data.clone().data();
    new_data.resize(5, 0);
    new_data[4] = 0x01; // overwrite vin length

    c.p1 = 0x00;
    c.p2 = 0x80;
    c.data = new_data.into();
    c
}

pub(crate) fn parse_sig(answer: &APDUAnswer) -> Result<coins_bip32::Signature, LedgerBTCError> {
    let mut sig = answer
        .data()
        .ok_or(LedgerBTCError::UnexpectedNullResponse)?
        .to_vec();
    sig[0] &= 0xfe;
    Ok(coins_bip32::Signature::try_from_der(&sig[..sig.len() - 1])
        .map_err(coins_bip32::Bip32Error::from)?)
}

pub(crate) fn should_sign(xpub: &DerivedXPub, signing_info: &[crate::app::SigningInfo]) -> bool {
    signing_info
        .iter()
        .filter(|s| s.deriv.is_some())  // filter no derivation
        .filter(|s| match s.prevout.script_pubkey.standard_type() {
            // filter SH types without spend scripts
            ScriptType::SH(_) | ScriptType::WSH(_) => {
                s.prevout.spend_script() != &SpendScript::Missing
            },
            _ => true
        })
        .any(|s| xpub.derivation.is_possible_ancestor_of(s.deriv.as_ref().unwrap()))
}
