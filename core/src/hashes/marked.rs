use std::io::Write;
use crate::ser::ByteFormat;

/// Marks a hash function digest.
pub trait Digest: Default + ByteFormat + Copy {}

/// A trait describing the interface for wrapped hashes. We wrap digests in this trait and name
/// them based on their function to prevent type-confusion between many different 32-byte digests.
pub trait MarkedDigest: Default + ByteFormat + Copy {
    /// The associated Digest type that is marked.
    type Digest: Digest;
    /// Wrap a digest of the appropriate type in the marker.
    fn new(hash: Self::Digest) -> Self;
    /// Return a copy of the internal digest.
    fn internal(&self) -> Self::Digest;
    /// Return the underlying bytes
    fn bytes(&self) -> Vec<u8>;
    /// Return a clone in reverse byte order
    fn reversed(&self) -> Self {
        let mut digest = self.bytes();
        digest.reverse();
        Self::read_from(&mut digest.as_slice(), 0).unwrap()
    }
}

/// An interface for a haser that can be written to. Parameterized by the digest that it outputs.
pub trait MarkedDigestWriter<T: Digest>: Default + Write {
    /// Consumes the hasher, calculates the digest from the written bytes. Returns a Digest
    /// of the parameterized type.
    fn finish(self) -> T;
    /// Calls finish, and wraps the result in a `MarkedDigest` type. Genericized to support any
    /// `MarkedDigest` that wraps the same parameterized type.
    fn finish_marked<M: MarkedDigest<Digest = T>>(self) -> M {
        MarkedDigest::new(self.finish())
    }
}
