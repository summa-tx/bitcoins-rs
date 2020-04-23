//! Transaction inputs, their components, and the input vector.

use js_sys;
use serde::ser::{Serialize, SerializeStruct, Serializer};
use wasm_bindgen::prelude::*;

use riemann_core::{ser::Ser, types::primitives::PrefixVec};
use rmn_btc::types::{script, txin};

use crate::{errors::WasmError, hashes::TXID};

wrap_struct!(
    /// An Outpoint. This is a unique identifier for a UTXO, and is composed of a transaction ID (in
    /// Bitcoin-style LE format), and the index of the output being spent within that transactions
    /// output vectour (vout).
    txin::BitcoinOutpoint
);
wrap_struct!(
    /// An TxInput. This data structure contains an outpoint referencing an existing UTXO, a
    /// `script_sig`, which will contain spend authorization information (when spending a Legacy or
    /// Witness-via-P2SH prevout), and a sequence number which may encode relative locktim semantics
    /// in version 2+ transactions.
    txin::BitcoinTxIn
);
wrap_struct!(
    /// A prefixed vector of `BitcoinTxIn`s.
    txin::Vin
);

impl_simple_getter!(BitcoinOutpoint, idx, u32);
impl_wrapped_getter!(BitcoinOutpoint, txid, TXID);

impl_simple_getter!(BitcoinTxIn, sequence, u32);
impl_wrapped_getter!(BitcoinTxIn, outpoint, BitcoinOutpoint);

impl_prefix_vec_access!(txin::Vin, txin::BitcoinTxIn);

#[wasm_bindgen]
impl BitcoinOutpoint {
    /// Returns the `default`, or `null` Outpoint. This is used in the coinbase input.
    pub fn null() -> Self {
        txin::BitcoinOutpoint::null().into()
    }

    /// Returns the `default`, or `null` Outpoint. This is used in the coinbase input.
    pub fn default() -> Self {
        txin::BitcoinOutpoint::null().into()
    }

    /// Returns a new Outpoint from a digest and index
    #[wasm_bindgen(constructor)]
    pub fn new(txid: &TXID, idx: u32) -> Self {
        txin::BitcoinOutpoint::new(txid.clone().into(), idx).into()
    }
}

#[wasm_bindgen]
impl BitcoinTxIn {
    /// Instantiate a new BitcoinTxIn.
    #[wasm_bindgen(constructor)]
    pub fn new(outpoint: BitcoinOutpoint, script_sig: &[u8], sequence: u32) -> Self {
        txin::BitcoinTxIn::new(outpoint.0, script::ScriptSig::from(script_sig), sequence).into()
    }

    /// Return the input's script sig.
    #[wasm_bindgen(method, getter)]
    pub fn script_sig(&self) -> js_sys::Uint8Array {
        js_sys::Uint8Array::from(self.0.script_sig.items())
    }
}
