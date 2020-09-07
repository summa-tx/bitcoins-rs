//! Pluggable standardized Bitcoin backend

#![forbid(unsafe_code)]
#![warn(missing_docs)]
#![warn(unused_extern_crates)]

#[doc(hidden)]
#[macro_use]
pub mod macros;

/// Bitcoin Provider trait
pub mod provider;

/// Pending Transaction
pub mod pending;

/// Outpoint spend watcher
pub mod watcher;

/// Chain watcher
pub mod chain;

#[doc(hidden)]
#[cfg(any(feature = "rpc", feature = "esplora"))]
pub mod reqwest_utils;

/// Utils
pub mod utils;

/// EsploraProvider
#[cfg(feature = "esplora")]
pub mod esplora;

/// Local (or remote) node RPC
#[cfg(feature = "rpc")]
pub mod rpc;

/// Common usage
pub mod prelude;

/// Minimal Types
pub mod types;

/// The default poll interval, set to 300 seconds (5 minutes)
pub const DEFAULT_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(180 * 1000);

/// The default number of cache items to keep in a caching provider
pub const DEFAULT_CACHE_SIZE: usize = 300;

// Alias the default encoder
type Encoder = bitcoins::Encoder;

// Useful alias for the stateful streams
#[cfg(target_arch = "wasm32")]
type ProviderFut<'a, T> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<T, crate::provider::ProviderError>> + 'a>,
>;

// Useful alias for the stateful streams
#[cfg(not(target_arch = "wasm32"))]
type ProviderFut<'a, T> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<T, crate::provider::ProviderError>> + 'a + Send>,
>;
