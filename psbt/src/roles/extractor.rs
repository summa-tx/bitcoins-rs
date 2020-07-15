use crate::{roles::PSTExtractor, PSBTError, PSBT, PST};
use riemann_core::builder::TxBuilder;
use rmn_bip32 as bip32;
use rmn_btc::{
    enc::encoder::BitcoinEncoderMarker,
    types::BitcoinTx,
};

/// An extractor
pub struct PSBTExtractor();

impl<A, E> PSTExtractor<A, PSBT<A, E>> for PSBTExtractor
where
    A: BitcoinEncoderMarker,
    E: bip32::enc::XKeyEncoder,
{
    type Error = PSBTError;

    fn extract(&mut self, pst: &PSBT<A, E>) -> Result<BitcoinTx, Self::Error> {
        // For convenience, we use a WitnessBuilder. If we ever set a witness, we return a witness
        // transaction. Otherwise we return a legacy transaction.
        let mut builder = pst.tx_builder()?;

        for (i, input_map) in pst.input_maps().iter().enumerate() {
            if !input_map.is_finalized() {
                return Err(PSBTError::UnfinalizedInput(i));
            }

            // Insert a script sig if we have one.
            if let Ok(script_sig) = input_map.finalized_script_sig() {
                builder = builder.set_script_sig(i, script_sig);
            }

            // Insert a witness if we have one, or an empty witness otherwise
            if let Ok(witness) = input_map.finalized_script_witness() {
                builder = builder.extend_witnesses(vec![witness]);
            } else {
                builder = builder.extend_witnesses(vec![Default::default()]);
            }
        }

        Ok(builder.build())
    }
}
