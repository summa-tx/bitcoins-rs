//! Holds Bitcoin specific types, including scripts, witnesses, inputs, outputs, and transactions.
//! Extends the `Transaction` trait to maintain a type distinction between Legacy and Witness
//! transactions (and allow conversion from one to the other).

pub mod script;
pub mod transactions;
pub mod txin;
pub mod txout;

pub use script::*;
pub use transactions::*;
pub use txin::*;
pub use txout::*;
