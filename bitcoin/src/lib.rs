//! This crate provides a simple interface for interacting with Bitcoin mainnet,
//! testnet, and signet.

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(unused_extern_crates)]

#[macro_use]
#[doc(hidden)]
#[cfg_attr(tarpaulin, skip)]
pub mod macros;

pub mod builder;
pub mod enc;
pub mod hashes;
pub mod nets;
pub mod types;

/// Common re-exports
pub mod prelude;

#[doc(hidden)]
#[cfg(any(feature = "mainnet", feature = "testnet"))]
pub mod defaults;

#[cfg(any(feature = "mainnet", feature = "testnet"))]
pub use defaults::network::{Encoder, Network};

pub use nets::*;
