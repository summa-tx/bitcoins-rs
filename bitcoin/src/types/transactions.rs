//! Bitcoin transaction types and associated sighash arguments.
use std::io::{Read, Write, Error as IOError};
use bitcoin_spv::types::{Hash256Digest};
use thiserror::Error;

use riemann_core::{
    hashes::{
        hash256::{Hash256Writer},
        marked::{MarkedDigest, MarkedDigestWriter},
    },
    ser::{Ser, SerError},
    types::{
        primitives::{PrefixVec},
        tx::{Transaction},
    },
};

use crate::{
    hashes::{TXID, WTXID},
    script::{Script, ScriptSig, Witness},
    txin::{BitcoinTxIn, Vin},
    txout::{TxOut, Vout},
};


/// An Error type for transaction objects
#[derive(Debug, Error)]
pub enum TxError{
    /// Serialization-related errors
    #[error(transparent)]
    SerError(#[from] SerError),

    /// IOError bubbled up from a `Write` passed to a `Ser::serialize` implementation.
    #[error(transparent)]
    IOError(#[from] IOError),

    /// Sighash NONE is unsupported
    #[error("SIGHASH_NONE is unsupported")]
    NoneUnsupported,

    /// Satoshi's sighash single bug. Throws an error here.
    #[error("SIGHASH_SINGLE bug is unsupported")]
    SighashSingleBug,

    /// Caller provided an unknown sighash type to `Sighash::from_u8`
    #[error("Unknown Sighash: {}", .0)]
    UnknownSighash(u8),

    /// Got an unknown flag where we expected a witness flag. May indicate a non-witness
    /// transaction.
    #[error("Witness flag not as expected. Got {:?}. Expected {:?}.", .0, [0u8, 1u8])]
    BadWitnessFlag([u8; 2]),

    // /// No inputs in vin
    // #[error("Vin may not be empty")]
    // EmptyVin,
    //
    // /// No outputs in vout
    // #[error("Vout may not be empty")]
    // EmptyVout
}

/// Type alias for result with TxError
pub type TxResult<T> = Result<T, TxError>;

/// Marker trait for BitcoinTransactions.
pub trait BitcoinTransaction<'a>: Transaction<'a> {}

/// Basic functionality for a Witness Transaction
///
/// This trait has been generalized to support transactions from Non-Bitcoin networks. The
/// transaction specificies which types it considers to be inputs and outputs, and a struct that
/// contains its Sighash arguments. This allows others to define custom transaction types with
/// unique functionality.
pub trait WitnessTransaction<'a>: BitcoinTransaction<'a> {
    /// The MarkedDigest type for the Transaction's Witness TXID
    type WTXID: MarkedDigest<Digest = Self::Digest>;
    /// The BIP143 sighash args needed to sign an input
    type WitnessSighashArgs;
    /// A type that represents this transactions per-input `Witness`.
    type Witness;

    /// Instantiate a new WitnessTx from the arguments.
    fn new<I, O, W>(
        version: u32,
        vin: I,
        vout: O,
        witnesses: W,
        locktime: u32
    ) -> Self
    where
        I: Into<Vec<Self::TxIn>>,
        O: Into<Vec<Self::TxOut>>,
        W: Into<Vec<Self::Witness>>;

    /// Calculates the witness txid of the transaction.
    fn wtxid(&self) -> Self::WTXID;

    /// Writes the Legacy sighash preimage to the provider writer.
    fn write_legacy_sighash_preimage<W: Write>(&self, writer: &mut W, args: &LegacySighashArgs) -> Result<(), Self::TxError>;

    /// Calculates the Legacy sighash preimage given the sighash args.
    fn legacy_sighash(&self, args: &LegacySighashArgs) -> Result<Self::Digest, Self::TxError> {
        let mut w = Self::HashWriter::default();
        self.write_legacy_sighash_preimage(&mut w, args)?;
        Ok(w.finish())
    }

    /// Writes the BIP143 sighash preimage to the provided `writer`. See the
    /// `WitnessSighashArgsSigh` documentation for more in-depth discussion of sighash.
    fn write_witness_sighash_preimage<W: Write>(&self, writer: &mut W, args: &Self::WitnessSighashArgs) -> Result<(), Self::TxError>;

    /// Calculates the BIP143 sighash given the sighash args. See the
    /// `WitnessSighashArgsSigh` documentation for more in-depth discussion of sighash.
    fn witness_sighash(&self, args: &Self::WitnessSighashArgs) -> Result<Self::Digest, Self::TxError> {
        let mut w = Self::HashWriter::default();
        self.write_witness_sighash_preimage(&mut w, args)?;
        Ok(w.finish())
    }

    /// Returns a reference to the transaction's witnesses.
    fn witnesses(&'a self) -> &'a[Self::Witness];
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// All possible Sighash modes
pub enum Sighash{
    /// Sign ALL inputs and ALL outputs
    All = 0x01,
    /// Sign ALL inputs and NO outputs (unsupported)
    None = 0x02,
    /// Sign ALL inputs and ONE output
    Single = 0x3,
    /// Sign ONE inputs and ALL outputs
    AllACP = 0x81,
    /// Sign ONE inputs and NO outputs (unsupported)
    NoneACP = 0x82,
    /// Sign ONE inputs and ONE output
    SingleACP = 0x83,
}

impl Sighash {
    /// Convert a u8 into a Sighash flag or an error.
    pub fn from_u8(flag: u8) -> Result<Sighash, TxError> {
        match flag {
            0x01 => Ok(Sighash::All),
            0x02 => Ok(Sighash::None),
            0x3 => Ok(Sighash::Single),
            0x81 => Ok(Sighash::AllACP),
            0x82 => Ok(Sighash::NoneACP),
            0x83 => Ok(Sighash::SingleACP),
            _ => Err(TxError::UnknownSighash(flag))
        }
    }
}

/// Arguments required to serialize the transaction to create the sighash digest.Used in
/// `legacy_sighash`to abstract the sighash serialization logic from the hasher used.
///
/// SIGHASH_ALL commits to ALL inputs, and ALL outputs. It indicates that no further modification
/// of the transaction is allowed without invalidating the signature.
///
/// SIGHASH_ALL + ANYONECANPAY commits to ONE input and ALL outputs. It indicates that anyone may
/// add additional value to the transaction, but that no one may modify the payments made. Any
/// extra value added above the sum of output values will be given to miners as part of the tx
/// fee.
///
/// SIGHASH_SINGLE commits to ALL inputs, and ONE output. It indicates that anyone may append
/// additional outputs to the transaction to reroute funds from the inputs. Additional inputs
/// cannot be added without invalidating the signature. It is logically difficult to use securely,
/// as it consents to funds being moved, without specifying their destination.
///
/// SIGHASH_SINGLE commits specifically the the output at the same index as the input being
/// signed. If there is no output at that index, (because, e.g. the input vector is longer than
/// the output vector) it behaves insecurely, and we do not implement that protocol bug.
///
/// SIGHASH_SINGLE + ANYONECANPAY commits to ONE input and ONE output. It indicates that anyone
/// may add additional value to the transaction, and route value to any other location. The
/// signed input and output must be included in the fully-formed transaction at the same index in
/// their respective vectors.
///
/// For Legacy sighash documentation, see here:
///
/// - https://en.bitcoin.it/wiki/OP_CHECKSIG#Hashtype_SIGHASH_ALL_.28default.29
///
/// # Note
///
/// After signing the digest, you MUST append the sighash indicator
/// byte to the resulting signature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LegacySighashArgs<'a> {
    /// The index of the input we'd like to sign
    pub index: usize,
    /// The sighash mode to use.
    pub sighash_flag: Sighash,
    /// The script used in the prevout, which must be signed. In complex cases involving
    /// `OP_CODESEPARATOR` this must be the subset of the script containing the `OP_CHECKSIG`
    /// currently being executed.
    pub prevout_script: &'a Script,
}

/// A Legacy (non-witness) Transaction.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct LegacyTx {
    /// The version number. Usually 1 or 2.
    version: u32,
    /// The vector of inputs
    vin: Vin,
    /// The vector of outputs
    vout: Vout,
    /// The nLocktime field.
    locktime: u32
}

impl LegacyTx {
    /// Performs steps 6, 7, and 8 of the sighash setup described here:
    /// https://en.bitcoin.it/wiki/OP_CHECKSIG#How_it_works
    /// https://bitcoin.stackexchange.com/questions/3374/how-to-redeem-a-basic-tx
    ///
    /// OP_CODESEPARATOR functionality is NOT provided here.
    ///
    /// TODO: memoize
    fn legacy_sighash_prep(&self, index: usize, prevout_script: &Script) -> Self
    {
        let mut copy_tx = self.clone();

        for i in 0..copy_tx.vin.len() {
            copy_tx.vin[i].script_sig = if i == index {
                ScriptSig::from(prevout_script.items())
            } else {
                ScriptSig::null()
            };
        };
        copy_tx
    }

    /// Modifies copy_tx according to legacy SIGHASH_SINGLE semantics.
    ///
    /// For Legacy sighash documentation, see here:
    ///
    /// - https://en.bitcoin.it/wiki/OP_CHECKSIG#Hashtype_SIGHASH_ALL_.28default.29
    fn legacy_sighash_single(
        copy_tx: &mut Self,
        index: usize) -> TxResult<()>
    {
        let mut tx_outs: Vec<TxOut> = (0..index).map(|_| TxOut::null()).collect();
        tx_outs.push(copy_tx.vout[index].clone());
        copy_tx.vout = Vout::new(tx_outs);

        let mut vin = vec![];

        // let mut vin = copy_tx.vin.clone();
        for i in 0..copy_tx.vin.items().len() {
            let mut txin = copy_tx.vin[i].clone();
            if i != index { txin.sequence = 0; }
            vin.push(txin);
        }
        copy_tx.vin = vin.into();
        Ok(())
    }

    /// Modifies copy_tx according to legacy SIGHASH_ANYONECANPAY semantics.
    ///
    /// For Legacy sighash documentation, see here:
    ///
    /// - https://en.bitcoin.it/wiki/OP_CHECKSIG#Hashtype_SIGHASH_ALL_.28default.29
    fn legacy_sighash_anyone_can_pay(
        copy_tx: &mut Self,
        index: usize) -> TxResult<()>
    {
        copy_tx.vin = Vin::new(vec![copy_tx.vin[index].clone()]);
        Ok(())
    }
}

impl<'a> Transaction<'a> for LegacyTx {
    type TxError = TxError;
    type Digest = Hash256Digest;
    type TxIn = BitcoinTxIn;
    type TxOut = TxOut;
    type SighashArgs = LegacySighashArgs<'a>;
    type TXID = TXID;
    type HashWriter = Hash256Writer;

    fn new<I, O>(
        version: u32,
        vin: I,
        vout: O,
        locktime: u32
    ) -> Self
    where
        I: Into<Vec<Self::TxIn>>,
        O: Into<Vec<Self::TxOut>>
    {
        Self{
            version,
            vin: Vin::from(vin),
            vout: Vout::from(vout),
            locktime,
        }
    }

    fn inputs(&'a self) -> &'a[Self::TxIn] {
        &self.vin.items()
    }

    fn outputs(&'a self) -> &'a[Self::TxOut] {
        &self.vout.items()
    }

    fn version(&self) -> u32 {
        self.version
    }

    fn locktime(&self) -> u32 {
        self.locktime
    }

    fn write_sighash_preimage<W: Write>(
        &self,
        writer: &mut W,
        args: &LegacySighashArgs
    ) -> TxResult<()> {
        if args.sighash_flag == Sighash::None || args.sighash_flag == Sighash::NoneACP {
            return Err(TxError::NoneUnsupported);
        }

        let mut copy_tx: Self = self.legacy_sighash_prep(args.index, args.prevout_script);
        if args.sighash_flag == Sighash::Single || args.sighash_flag == Sighash::SingleACP {
            if args.index >= self.outputs().len() { return Err(TxError::SighashSingleBug); }
            Self::legacy_sighash_single(
                &mut copy_tx,
                args.index
            )?;
        }

        if args.sighash_flag as u8 & 0x80 == 0x80 {
            Self::legacy_sighash_anyone_can_pay(&mut copy_tx, args.index)?;
        }

        copy_tx.serialize(writer)?;
        Self::write_u32_le(writer, args.sighash_flag as u32)?;

        Ok(())
    }
}

impl<'a> BitcoinTransaction<'a> for LegacyTx {}

impl Ser for LegacyTx {
    type Error = TxError;

    fn to_json(&self) -> String {
        format!(
            "{{\"version\": {}, \"vin\": {}, \"vout\": {}, \"locktime\": {}}}",
            self.version,
            self.vin.to_json(),
            self.vout.to_json(),
            self.locktime
        )
    }

    fn serialized_length(&self) -> usize {
        let mut len = 4; // version
        len += self.vin.serialized_length();
        len += self.vout.serialized_length();
        len += 4; // locktime
        len
    }

    fn deserialize<R>(reader: &mut R, _limit: usize) -> Result<Self, Self::Error>
    where
        R: Read,
        Self: std::marker::Sized
    {
        let version = Self::read_u32_le(reader)?;
        let vin = Vin::deserialize(reader, 0)?;
        let vout = Vout::deserialize(reader, 0)?;
        let locktime = Self::read_u32_le(reader)?;
        Ok(Self{
            version,
            vin,
            vout,
            locktime,
        })
    }

    fn serialize<W>(&self, writer: &mut W) -> Result<usize, Self::Error>
    where
        W: Write
    {
        let mut len = Self::write_u32_le(writer, self.version())?;
        len += self.vin.serialize(writer)?;
        len += self.vout.serialize(writer)?;
        len += Self::write_u32_le(writer, self.locktime())?;
        Ok(len)
    }
}

/// Arguments required to serialize the transaction to create the BIP143 (witness) sighash
/// digest. Used in `witness_sighash` to abstract the sighash serialization logic from the hash
/// used.
///
/// SIGHASH_ALL commits to ALL inputs, and ALL outputs. It indicates that no further modification
/// of the transaction is allowed without invalidating the signature.
///
/// SIGHASH_ALL + ANYONECANPAY commits to ONE input and ALL outputs. It indicates that anyone may
/// add additional value to the transaction, but that no one may modify the payments made. Any
/// extra value added above the sum of output values will be given to miners as part of the tx
/// fee.
///
/// SIGHASH_SINGLE commits to ALL inputs, and ONE output. It indicates that anyone may append
/// additional outputs to the transaction to reroute funds from the inputs. Additional inputs
/// cannot be added without invalidating the signature. It is logically difficult to use securely,
/// as it consents to funds being moved, without specifying their destination.
///
/// SIGHASH_SINGLE commits specifically the the output at the same index as the input being
/// signed. If there is no output at that index, (because, e.g. the input vector is longer than
/// the output vector) it behaves insecurely, and we do not implement that protocol bug.
///
/// SIGHASH_SINGLE + ANYONECANPAY commits to ONE input and ONE output. It indicates that anyone
/// may add additional value to the transaction, and route value to any other location. The
/// signed input and output must be included in the fully-formed transaction at the same index in
/// their respective vectors.
///
/// For BIP143 sighash documentation, see here:
///
/// - https://github.com/bitcoin/bips/blob/master/bip-0143.mediawiki
///
/// # Note
///
/// After signing the digest, you MUST append the sighash indicator byte to the resulting
/// signature.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WitnessSighashArgs<'a> {
    /// The index of the input we'd like to sign
    pub index: usize,
    /// The sighash mode to use.
    pub sighash_flag: Sighash,
    /// The script used in the prevout, which must be signed. In complex cases involving
    /// `OP_CODESEPARATOR` this must be the subset of the script containing the `OP_CHECKSIG`
    /// currently being executed.
    pub prevout_script: &'a Script,
    /// The value of the prevout.
    pub prevout_value: u64,
}

/// A witness transaction. Any transaction that contains 1 or more witnesses.
#[derive(Clone, Debug, Eq, PartialEq, Default)]
pub struct WitnessTx {
    legacy_tx: LegacyTx,
    witnesses: Vec<Witness>,
}

impl WitnessTx {
    /// Returns a legacy transaction with identical properties (less witnesses).
    pub fn without_witness(&self) -> LegacyTx {
        self.legacy_tx.clone()
    }

    /// Calculates `hash_prevouts` according to BIP143 semantics.`
    ///
    /// For BIP143 (Witness and Compatibility sighash) documentation, see here:
    ///
    /// - https://github.com/bitcoin/bips/blob/master/bip-0143.mediawiki
    ///
    /// TODO: memoize
    fn hash_prevouts(&self, sighash_flag: Sighash) -> TxResult<Hash256Digest> {
        if sighash_flag as u8 & 0x80 == 0x80 {
            Ok(Hash256Digest::default())
        } else {
            let mut w = Hash256Writer::default();
            for input in self.legacy_tx.vin.items().iter() {
                input.outpoint.serialize(&mut w)?;
            }
            Ok(w.finish())
        }

    }

    /// Calculates `hash_sequence` according to BIP143 semantics.`
    ///
    /// For BIP143 (Witness and Compatibility sighash) documentation, see here:
    ///
    /// - https://github.com/bitcoin/bips/blob/master/bip-0143.mediawiki
    ///
    /// TODO: memoize
    fn hash_sequence(&self, sighash_flag: Sighash) -> TxResult<Hash256Digest> {
        if sighash_flag == Sighash::Single || sighash_flag as u8 & 0x80 == 0x80 {
            Ok(Hash256Digest::default())
        } else {
            let mut w = Hash256Writer::default();
            for input in self.legacy_tx.vin.items().iter() {
                Self::write_u32_le(&mut w, input.sequence)?;
            }
            Ok(w.finish())
        }
    }

    /// Calculates `hash_outputs` according to BIP143 semantics.`
    ///
    /// For BIP143 (Witness and Compatibility sighash) documentation, see here:
    ///
    /// - https://github.com/bitcoin/bips/blob/master/bip-0143.mediawiki
    ///
    /// TODO: memoize
    fn hash_outputs(&self, index: usize, sighash_flag: Sighash) -> TxResult<Hash256Digest> {
        match sighash_flag {
            Sighash::All | Sighash::AllACP  => {
                let mut w = Hash256Writer::default();
                for output in self.legacy_tx.vout.items().iter() {
                    output.serialize(&mut w)?;
                }
                Ok(w.finish())
            },
            Sighash::Single | Sighash::SingleACP => {
                let mut w = Hash256Writer::default();
                self.legacy_tx.vout[index].serialize(&mut w)?;
                Ok(w.finish())
            },
            _ => Ok(Hash256Digest::default())
        }
    }
}

impl<'a> Transaction<'a> for WitnessTx {
    type TxError = TxError;
    type Digest = Hash256Digest;
    type TxIn = BitcoinTxIn;
    type TxOut = TxOut;
    type SighashArgs = WitnessSighashArgs<'a>;
    type TXID = TXID;
    type HashWriter = Hash256Writer;

    fn new<I, O>(
        version: u32,
        vin: I,
        vout: O,
        locktime: u32
    ) -> Self
    where
        I: Into<Vec<Self::TxIn>>,
        O: Into<Vec<Self::TxOut>>
    {
        let input_vector: Vec<BitcoinTxIn> = vin.into();
        let witnesses = input_vector.iter().map(|_| Witness::null()).collect();

        let legacy_tx = LegacyTx::new(version, input_vector, vout, locktime);
        Self{
            legacy_tx,
            witnesses
        }
    }

    fn inputs(&'a self) -> &'a[Self::TxIn] {
        &self.legacy_tx.vin.items()
    }

    fn outputs(&'a self) -> &'a[Self::TxOut] {
        &self.legacy_tx.vout.items()
    }

    fn version(&self) -> u32 {
        self.legacy_tx.version
    }

    fn locktime(&self) -> u32 {
        self.legacy_tx.locktime
    }

    // Override the txid method to exclude witnesses
    fn txid(&self) -> Self::TXID {
        let mut w = Self::HashWriter::default();
        Self::write_u32_le(&mut w, self.version()).expect("No IOError from SHA2");
        self.legacy_tx.vin.serialize(&mut w).expect("No IOError from SHA2");
        self.legacy_tx.vout.serialize(&mut w).expect("No IOError from SHA2");
        Self::write_u32_le(&mut w, self.locktime()).expect("No IOError from SHA2");
        w.finish_marked()
    }

    fn write_sighash_preimage<W: Write>(
        &self,
        writer: &mut W,
        args: &Self::SighashArgs,
    ) -> TxResult<()> {
        self.write_witness_sighash_preimage(writer, args)
    }
}

impl<'a> BitcoinTransaction<'a> for WitnessTx {}

impl<'a> WitnessTransaction<'a> for WitnessTx {
    type WTXID = WTXID;
    type WitnessSighashArgs = WitnessSighashArgs<'a>;
    type Witness = Witness;

    fn new<I, O, W>(
        version: u32,
        vin: I,
        vout: O,
        witnesses: W,
        locktime: u32
    ) -> Self
    where
        I: Into<Vec<Self::TxIn>>,
        O: Into<Vec<Self::TxOut>>,
        W: Into<Vec<Self::Witness>>
    {
        let legacy_tx = LegacyTx::new(
            version,
            vin,
            vout,
            locktime,
        );
        Self{
            legacy_tx,
            witnesses: witnesses.into()
        }
    }

    fn wtxid(&self) -> Self::WTXID {
        let mut w = Self::HashWriter::default();
        self.serialize(&mut w).expect("No IOError from SHA2");
        w.finish_marked()
    }

    fn write_legacy_sighash_preimage<W: Write>(&self, writer: &mut W, args: &LegacySighashArgs) -> Result<(), Self::TxError> {
        self.legacy_tx.write_sighash_preimage(writer, args)
    }

    fn write_witness_sighash_preimage<W>(
        &self,
        writer: &mut W,
        args: &WitnessSighashArgs) -> TxResult<()>
    where
        W: Write
    {
        if args.sighash_flag == Sighash::None || args.sighash_flag == Sighash::NoneACP {
            return Err(TxError::NoneUnsupported);
        }

        if (args.sighash_flag == Sighash::Single || args.sighash_flag == Sighash::SingleACP) &&
            args.index >= self.outputs().len()
        {
            return Err(TxError::SighashSingleBug)
        }

        let input = &self.legacy_tx.vin[args.index];

        Self::write_u32_le(writer, self.legacy_tx.version)?;
        self.hash_prevouts(args.sighash_flag)?.serialize(writer)?;
        self.hash_sequence(args.sighash_flag)?.serialize(writer)?;
        input.outpoint.serialize(writer)?;
        args.prevout_script.serialize(writer)?;
        Self::write_u64_le(writer, args.prevout_value)?;
        Self::write_u32_le(writer, input.sequence)?;
        self.hash_outputs(args.index, args.sighash_flag)?.serialize(writer)?;
        Self::write_u32_le(writer, self.legacy_tx.locktime)?;
        Self::write_u32_le(writer, args.sighash_flag as u32)?;
        Ok(())
    }

    fn witnesses(&'a self) -> &'a[Self::Witness] {
        &self.witnesses
    }
}

impl Ser for WitnessTx {
    type Error = TxError;

    fn to_json(&self) -> String {
        format!(
            "{{\"version\": {}, \"vin\": {}, \"vout\": {}, \"witnesses\": {}, \"locktime\": {}}}",
            self.version(),
            self.legacy_tx.vin.to_json(),
            self.legacy_tx.vout.to_json(),
            self.witnesses.to_json(),
            self.locktime()
        )
    }


    fn serialized_length(&self) -> usize {
        let mut len = 4; // version
        len += 2;  // Segwit Flag
        len += self.legacy_tx.vin.serialized_length();
        len += self.legacy_tx.vout.serialized_length();
        len += self.witnesses.serialized_length();
        len += 4; // locktime
        len
    }

    fn deserialize<R>(reader: &mut R, _limit: usize) -> Result<Self, Self::Error>
    where
        R: Read,
        Self: std::marker::Sized
    {
        let version = Self::read_u32_le(reader)?;
        let mut flag = [0u8; 2];
        reader.read_exact(&mut flag)?;
        if flag != [0u8, 1u8] { return Err(TxError::BadWitnessFlag(flag)); };
        let vin = Vin::deserialize(reader, 0)?;
        let vout = Vout::deserialize(reader, 0)?;
        let witnesses = Vec::<Witness>::deserialize(reader, vin.len())?;
        let locktime = Self::read_u32_le(reader)?;

        let legacy_tx = LegacyTx{
            version,
            vin,
            vout,
            locktime,
        };

        Ok(Self{
            legacy_tx,
            witnesses,
        })
    }

    fn serialize<W>(&self, writer: &mut W) -> Result<usize, Self::Error>
    where
        W: Write
    {
        let mut len = Self::write_u32_le(writer, self.version())?;
        len += writer.write(&[0u8, 1u8])?;
        len += self.legacy_tx.vin.serialize(writer)?;
        len += self.legacy_tx.vout.serialize(writer)?;
        len += self.witnesses.serialize(writer)?;
        len += Self::write_u32_le(writer, self.locktime())?;
        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_calculates_legacy_sighashes_and_txids() {
        // pulled from riemann helpers
        let tx_hex = "0100000001813f79011acb80925dfe69b3def355fe914bd1d96a3f5f71bf8303c6a989c7d1000000006b483045022100ed81ff192e75a3fd2304004dcadb746fa5e24c5031ccfcf21320b0277457c98f02207a986d955c6e0cb35d446a89d3f56100f4d7f67801c31967743a9c8e10615bed01210349fc4e631e3624a545de3f89f5d8684c7b8138bd94bdd531d2e213bf016b278afeffffff02a135ef01000000001976a914bc3b654dca7e56b04dca18f2566cdaf02e8d9ada88ac99c39800000000001976a9141c4bc762dd5423e332166702cb75f40df79fea1288ac19430600";
        let tx = LegacyTx::deserialize_hex(tx_hex.to_owned()).unwrap();

        let prevout_script_hex = "17a91424d6008f143af0cca57344069c46661aa4fcea2387";
        let prevout_script = Script::deserialize_hex(prevout_script_hex.to_owned()).unwrap();

        let all = Hash256Digest::deserialize_hex("b85c4f8d1377cc138225dd9b319d0a4ca547f7884270640f44c5fcdf269e0fe8".to_owned()).unwrap();
        let all_anyonecanpay = Hash256Digest::deserialize_hex("3b67a5114cc9fc837ddd6f6ec11bde38db5f68c34ab6ece2a043d7b25f2cf8bb".to_owned()).unwrap();
        let single = Hash256Digest::deserialize_hex("1dab67d768be0380fc800098005d1f61744ffe585b0852f8d7adc12121a86938".to_owned()).unwrap();
        let single_anyonecanpay = Hash256Digest::deserialize_hex("d4687b93c0a9090dc0a3384cd3a594ce613834bb37abc56f6032e96c597547e3".to_owned()).unwrap();

        let txid = Hash256Digest::deserialize_hex("03ee4f7a4e68f802303bc659f8f817964b4b74fe046facc3ae1be4679d622c45".to_owned()).unwrap();
        assert_eq!(tx.txid(), txid.into());

        let mut args = LegacySighashArgs {
            index: 0,
            sighash_flag: Sighash::All,
            prevout_script: &prevout_script,
        };

        assert_eq!(tx.sighash(&args).unwrap(), all);
        args.sighash_flag = Sighash::AllACP;
        assert_eq!(tx.sighash(&args).unwrap(), all_anyonecanpay);
        args.sighash_flag = Sighash::Single;
        assert_eq!(tx.sighash(&args).unwrap(), single);
        args.sighash_flag = Sighash::SingleACP;
        assert_eq!(tx.sighash(&args).unwrap(), single_anyonecanpay);
    }

    #[test]
    fn it_calculates_witness_sighashes_and_txids() {
        // pulled from riemann helpers
        let tx_hex = "02000000000101ee9242c89e79ab2aa537408839329895392b97505b3496d5543d6d2f531b94d20000000000fdffffff0173d301000000000017a914bba5acbec4e6e3374a0345bf3609fa7cfea825f18700cafd0700";
        let tx = WitnessTx::deserialize_hex(tx_hex.to_owned()).unwrap();

        let prevout_script_hex = "160014758ce550380d964051086798d6546bebdca27a73";
        let prevout_script = Script::deserialize_hex(prevout_script_hex.to_owned()).unwrap();

        let all = Hash256Digest::deserialize_hex("135754ab872e4943f7a9c30d6143c4c7187e33d0f63c75ec82a7f9a15e2f2d00".to_owned()).unwrap();
        let all_anyonecanpay = Hash256Digest::deserialize_hex("cc7438d5b15e93ba612dcd227cf1937c35273675b3aa7d1b771573667376ddf6".to_owned()).unwrap();
        let single = Hash256Digest::deserialize_hex("d04631d2742e6fd8e80e2e4309dece65becca41d37fd6bc0bcba041c52d824d5".to_owned()).unwrap();
        let single_anyonecanpay = Hash256Digest::deserialize_hex("ffea9cdda07170af9bc9967cedf485e9fe15b78a622e0c196c0b6fc64f40c615".to_owned()).unwrap();

        let txid = Hash256Digest::deserialize_hex("9e77087321b870859ebf08976d665c42d9f98cad18fff6a05a91c1d2da6d6c41".to_owned()).unwrap();
        assert_eq!(tx.txid(), txid.into());

        let mut args = WitnessSighashArgs {
            index: 0,
            sighash_flag: Sighash::All,
            prevout_script: &prevout_script,
            prevout_value: 120000,
        };

        assert_eq!(tx.sighash(&args).unwrap(), all);

        args.sighash_flag = Sighash::AllACP;
        assert_eq!(tx.sighash(&args).unwrap(), all_anyonecanpay);

        args.sighash_flag = Sighash::Single;
        assert_eq!(tx.sighash(&args).unwrap(), single);

        args.sighash_flag = Sighash::SingleACP;
        assert_eq!(tx.sighash(&args).unwrap(), single_anyonecanpay);
    }

    #[test]
    fn it_passes_more_witness_sighash_tests() {
        // from riemann
        let tx_hex = "02000000000102ee9242c89e79ab2aa537408839329895392b97505b3496d5543d6d2f531b94d20000000000fdffffffee9242c89e79ab2aa537408839329895392b97505b3496d5543d6d2f531b94d20000000000fdffffff0273d301000000000017a914bba5acbec4e6e3374a0345bf3609fa7cfea825f18773d301000000000017a914bba5acbec4e6e3374a0345bf3609fa7cfea825f1870000cafd0700";
        let tx = WitnessTx::deserialize_hex(tx_hex.to_owned()).unwrap();

        let prevout_script_hex = "160014758ce550380d964051086798d6546bebdca27a73";
        let prevout_script = Script::deserialize_hex(prevout_script_hex.to_owned()).unwrap();

        let all = Hash256Digest::deserialize_hex("75385c87ece4980b581cfd71bc5814f607801a87f6e0973c63dc9fda465c19c4".to_owned()).unwrap();
        let all_anyonecanpay = Hash256Digest::deserialize_hex("bc55c4303c82cdcc8e290c597a00d662ab34414d79ec15d63912b8be7fe2ca3c".to_owned()).unwrap();
        let single = Hash256Digest::deserialize_hex("9d57bf7af01a4e0baa57e749aa193d37a64e3bbc08eb88af93944f41af8dfc70".to_owned()).unwrap();
        let single_anyonecanpay = Hash256Digest::deserialize_hex("ffea9cdda07170af9bc9967cedf485e9fe15b78a622e0c196c0b6fc64f40c615".to_owned()).unwrap();

        let txid = Hash256Digest::deserialize_hex("184e7bce099679b27ed958213c97d2fb971e227c6517bca11f06ccbb97dcdc30".to_owned()).unwrap();
        assert_eq!(tx.txid(), txid.into());

        let mut args = WitnessSighashArgs {
            index: 1,
            sighash_flag: Sighash::All,
            prevout_script: &prevout_script,
            prevout_value: 120000,
        };

        assert_eq!(tx.sighash(&args).unwrap(), all);

        args.sighash_flag = Sighash::AllACP;
        assert_eq!(tx.sighash(&args).unwrap(), all_anyonecanpay);

        args.sighash_flag = Sighash::Single;
        assert_eq!(tx.sighash(&args).unwrap(), single);

        args.sighash_flag = Sighash::SingleACP;
        assert_eq!(tx.sighash(&args).unwrap(), single_anyonecanpay);
    }

    #[test]
    fn it_passes_more_legacy_sighash_tests() {
        // from riemann
        let tx_hex = "0200000002ee9242c89e79ab2aa537408839329895392b97505b3496d5543d6d2f531b94d20000000000fdffffffee9242c89e79ab2aa537408839329895392b97505b3496d5543d6d2f531b94d20000000000fdffffff0273d301000000000017a914bba5acbec4e6e3374a0345bf3609fa7cfea825f18773d301000000000017a914bba5acbec4e6e3374a0345bf3609fa7cfea825f18700000000";
        let tx = LegacyTx::deserialize_hex(tx_hex.to_owned()).unwrap();

        let prevout_script_hex = "160014758ce550380d964051086798d6546bebdca27a73";
        let prevout_script = Script::deserialize_hex(prevout_script_hex.to_owned()).unwrap();

        let all = Hash256Digest::deserialize_hex("3ab40bf1287b7be9a5c67ed0f97f80b38c5f68e53ec93bffd3893901eaaafdb2".to_owned()).unwrap();
        let all_anyonecanpay = Hash256Digest::deserialize_hex("2d5802fed31e1ef6a857346cc0a9085ea452daeeb3a0b5afcb16a2203ce5689d".to_owned()).unwrap();
        let single = Hash256Digest::deserialize_hex("ea52b62b26c1f0db838c952fa50806fb8e39ba4c92a9a88d1b4ba7e9c094517d".to_owned()).unwrap();
        let single_anyonecanpay = Hash256Digest::deserialize_hex("9e2aca0a04afa6e1e5e00ff16b06a247a0da1e7bbaa7cd761c066a82bb3b07d0".to_owned()).unwrap();

        let txid = Hash256Digest::deserialize_hex("40157948972c5c97a2bafff861ee2f8745151385c7f9fbd03991ddf59b76ac81".to_owned()).unwrap();
        assert_eq!(tx.txid(), txid.into());

        let mut args = LegacySighashArgs {
            index: 1,
            sighash_flag: Sighash::All,
            prevout_script: &prevout_script,
        };

        assert_eq!(tx.sighash(&args).unwrap(), all);

        args.sighash_flag = Sighash::AllACP;
        assert_eq!(tx.sighash(&args).unwrap(), all_anyonecanpay);

        args.sighash_flag = Sighash::Single;
        assert_eq!(tx.sighash(&args).unwrap(), single);

        args.sighash_flag = Sighash::SingleACP;
        assert_eq!(tx.sighash(&args).unwrap(), single_anyonecanpay);
    }
}
