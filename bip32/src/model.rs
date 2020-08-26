use std::convert::TryInto;

use coins_core::hashes::{Hash160Digest, Hash256, Hash256Digest, MarkedDigest};

use crate::{
    curve::model::{PointSerialize, RecoverableSigSerialize, ScalarSerialize, Secp256k1Backend},
    path::{DerivationPath, KeyDerivation},
    primitives::{ChainCode, Hint, KeyFingerprint, XKeyInfo},
    Bip32Error,
};

/// Any type that has access to a Secp256k1 backend.
pub trait HasBackend<'a, T: Secp256k1Backend> {
    /// Set the backend. Useful if you have created a backend after making a key with a `None`
    /// backend.
    fn set_backend(&mut self, backend: &'a T);

    /// Return a reference to the associated backend
    fn backend(&self) -> Result<&'a T, Bip32Error>;
}

/// Any type that contains a private key.
pub trait HasPrivkey<'a, T: Secp256k1Backend> {
    /// Return the associated private key
    fn privkey(&self) -> &T::Privkey;

    /// Return the 32 byte scalar as an array
    fn privkey_bytes(&self) -> [u8; 32] {
        self.privkey().privkey_array()
    }
}

/// Any type that contains a public key
pub trait HasPubkey<'a, T: Secp256k1Backend> {
    /// Return the associated public key
    fn pubkey(&self) -> &T::Pubkey;

    /// Return the associated public key in its compressed representation
    fn pubkey_bytes(&self) -> [u8; 33] {
        self.pubkey().pubkey_array()
    }

    /// Calculate the key fingerprint of the associated public key. This is the first 4 bytes of
    /// the Bitcoin HASH_160 of the compressed representation of the public key.
    fn fingerprint(&self) -> KeyFingerprint {
        self.pubkey().fingerprint()
    }

    /// Calculate the hash160 of the associated public key. This is commonly used to consturct
    /// pubkeyhash outputs in bitcoin-like chains, and has been provided here as a convenience.
    fn pubkey_hash160(&self) -> Hash160Digest {
        self.pubkey().hash160()
    }
}

/// Any type that has a private key and a backend may derive a public key.
///
/// This is generically implemented for any type that implements `HasPrivkey` and `HasBackend`.
pub trait CanDerivePubkey<'a, T: 'a + Secp256k1Backend>:
    HasPrivkey<'a, T> + HasBackend<'a, T>
{
    /// Derive the public key. Note that this operation may fail if no backend is found. This
    /// call performs a scalar multiplication, so should be cached if possible.
    fn derive_pubkey(&self) -> Result<T::Pubkey, Bip32Error> {
        Ok(self.backend()?.derive_pubkey(&self.privkey()))
    }

    /// Derive the public key. Note that this operation may fail if no backend is found. This
    /// call performs a scalar multiplication, so should be cached if possible.
    fn derive_pubkey_bytes(&self) -> Result<[u8; 33], Bip32Error> {
        Ok(self.derive_pubkey()?.pubkey_array())
    }

    /// Derive the public key's fingerprint. Note that this operation may fail if no backend is
    /// found. This call performs a scalar multiplication, so should be cachedif possible.
    fn derive_fingerprint(&self) -> Result<KeyFingerprint, Bip32Error> {
        Ok(self.derive_pubkey()?.fingerprint())
    }
}

impl<'a, T, K> CanDerivePubkey<'a, T> for K
where
    T: 'a + Secp256k1Backend,
    K: HasPrivkey<'a, T> + HasBackend<'a, T>,
{
}

/// Any type that has a private key and a backend may derive a public key
pub trait SigningKey<'a, T: 'a + Secp256k1Backend>:
    CanDerivePubkey<'a, T> + std::marker::Sized
{
    /// The corresponding verifying key
    type VerifyingKey: VerifyingKey<'a, T, SigningKey = Self>;

    /// Derive the corresponding pubkey
    fn derive_verifying_key(&self) -> Result<Self::VerifyingKey, Bip32Error>;

    /// Sign a digest
    fn sign_digest(&self, digest: Hash256Digest) -> Result<T::Signature, Bip32Error> {
        Ok(self.backend()?.sign_digest(&self.privkey(), digest))
    }

    /// Sign a digest and produce a recovery ID
    fn sign_digest_recoverable(
        &self,
        digest: Hash256Digest,
    ) -> Result<T::RecoverableSignature, Bip32Error> {
        Ok(self
            .backend()?
            .sign_digest_recoverable(&self.privkey(), digest))
    }

    /// Sign a message
    fn sign_with_hash<D>(&self, message: &[u8]) -> Result<T::Signature, Bip32Error>
    where
        D: MarkedDigest<Hash256Digest>,
    {
        self.sign_digest(D::digest_marked(message))
    }

    /// Sign a message and produce a recovery ID
    fn sign_recoverable_with_hash<D>(
        &self,
        message: &[u8],
    ) -> Result<T::RecoverableSignature, Bip32Error>
    where
        D: MarkedDigest<Hash256Digest>,
    {
        self.sign_digest_recoverable(D::digest_marked(message))
    }

    /// Produce a signature on `sha2(sha2(message))`
    fn sign(&self, message: &[u8]) -> Result<T::Signature, Bip32Error> {
        self.sign_with_hash::<Hash256>(message)
    }

    /// Produce a recoverable signature on `sha2(sha2(message))`
    fn sign_recoverable(&self, message: &[u8]) -> Result<T::RecoverableSignature, Bip32Error> {
        self.sign_recoverable_with_hash::<Hash256>(message)
    }
}

/// Any type that has a pubkey and a backend can verify signatures.
pub trait VerifyingKey<'a, T: 'a + Secp256k1Backend>:
    HasPubkey<'a, T> + HasBackend<'a, T> + std::marker::Sized
{
    /// The corresponding signing key type.
    type SigningKey: SigningKey<'a, T, VerifyingKey = Self>;

    /// Instantiate `Self` from the corresponding signing key
    fn from_signing_key(key: &Self::SigningKey) -> Result<Self, Bip32Error> {
        key.derive_verifying_key()
    }

    /// Verify a signature on a digest
    fn verify_digest(&self, digest: Hash256Digest, sig: &T::Signature) -> Result<(), Bip32Error> {
        self.backend()?
            .verify_digest(&self.pubkey(), digest, sig)
            .map_err(Into::into)
    }

    /// Verify a recoverable signature on a digest.
    fn verify_digest_recoverable(
        &self,
        digest: Hash256Digest,
        sig: &T::RecoverableSignature,
    ) -> Result<(), Bip32Error> {
        self.backend()?
            .verify_digest_recoverable(&self.pubkey(), digest, sig)
            .map_err(Into::into)
    }

    /// Verify a signature on a message
    fn verify_with_hash<D>(&self, message: &[u8], sig: &T::Signature) -> Result<(), Bip32Error>
    where
        D: MarkedDigest<Hash256Digest>,
    {
        self.verify_digest(D::digest_marked(message), sig)
    }

    /// Verify a recoverable signature on a message.
    fn verify_recoverable_with_hash<D>(
        &self,
        message: &[u8],
        sig: &T::RecoverableSignature,
    ) -> Result<(), Bip32Error>
    where
        D: MarkedDigest<Hash256Digest>,
    {
        self.verify_digest(D::digest_marked(message), &sig.without_recovery())
    }

    /// Produce a signature on `sha2(sha2(message))`
    fn verify(&self, message: &[u8], sig: &T::Signature) -> Result<(), Bip32Error> {
        self.verify_with_hash::<Hash256>(message, sig)
    }

    /// Produce a recoverable signature on `sha2(sha2(message))`
    fn verify_recoverable(
        &self,
        message: &[u8],
        sig: &T::RecoverableSignature,
    ) -> Result<(), Bip32Error> {
        self.verify_recoverable_with_hash::<Hash256>(message, sig)
    }
}

#[doc(hidden)]
pub trait HasXKeyInfo {
    /// Return the `XKeyInfo` object associated with the key
    fn xkey_info(&self) -> &XKeyInfo;
}

/// Extended Key common features
pub trait XKey: std::marker::Sized + Clone {
    /// Get the key's depth
    fn depth(&self) -> u8;
    /// Get the key's parent
    fn parent(&self) -> KeyFingerprint;
    /// Get the key's index
    fn index(&self) -> u32;
    /// Get the key's chain_code
    fn chain_code(&self) -> ChainCode;
    /// Get the key's hint
    fn hint(&self) -> Hint;
}

impl<T: HasXKeyInfo + std::marker::Sized + Clone> XKey for T {
    fn depth(&self) -> u8 {
        self.xkey_info().depth
    }
    fn parent(&self) -> KeyFingerprint {
        self.xkey_info().parent
    }
    fn index(&self) -> u32 {
        self.xkey_info().index
    }
    fn chain_code(&self) -> ChainCode {
        self.xkey_info().chain_code
    }
    fn hint(&self) -> Hint {
        self.xkey_info().hint
    }
}

/// A trait for extended keys which can derive private children
///
/// This is generically implemented for any type that implements `SigningKey` and
/// `DerivePrivateChild`
pub trait DerivePrivateChild<'a, T: Secp256k1Backend>: XKey + HasPrivkey<'a, T> {
    /// Derive a child privkey
    fn derive_private_child(&self, index: u32) -> Result<Self, Bip32Error>;

    /// Derive a series of child indices. Allows traversing several levels of the tree at once.
    /// Accepts an iterator producing u32, or a string.
    fn derive_private_path<E, P>(&self, p: P) -> Result<Self, Bip32Error>
    where
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        let path: DerivationPath = p.try_into().map_err(Into::into)?;

        if path.is_empty() {
            return Ok(self.to_owned());
        }

        let mut current = self.to_owned();
        for index in path.iter() {
            current = current.derive_private_child(*index)?;
        }
        Ok(current)
    }
}

/// A trait for extended keys which can derive public children.
///
/// This is generically implemented for any type that implements `VerifyingKey` and
/// `DerivePublicChild`
pub trait DerivePublicChild<'a, T: Secp256k1Backend>: XKey + HasPubkey<'a, T> {
    /// Derive a child pubkey
    fn derive_public_child(&self, index: u32) -> Result<Self, Bip32Error>;

    /// Derive a series of child indices. Allows traversing several levels of the tree at once.
    /// Accepts an iterator producing u32, or a string.
    fn derive_public_path<E, P>(&self, p: P) -> Result<Self, Bip32Error>
    where
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        let path: DerivationPath = p.try_into().map_err(Into::into)?;

        if path.is_empty() {
            return Ok(self.to_owned());
        }

        if let (_, Some(_)) = path.last_hardened() {
            return Err(Bip32Error::HardenedDerivationFailed);
        }

        let mut current = self.to_owned();
        for index in path.iter() {
            current = current.derive_public_child(*index)?;
        }
        Ok(current)
    }
}

/// Shortcuts for deriving and signing.
///
/// This trait is implemented on all types that impl `DerivePublicChild` and `VerifyingKey`
pub trait XSigning<'a, T: 'a + Secp256k1Backend>:
    DerivePrivateChild<'a, T> + SigningKey<'a, T>
{
    /// Derive a descendant, and have it sign a digest
    fn descendant_sign_digest<E, P>(
        &self,
        path: P,
        digest: Hash256Digest,
    ) -> Result<T::Signature, Bip32Error>
    where
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.derive_private_path(path)?.sign_digest(digest)
    }

    /// Derive a descendant, and have it sign a digest and produce a recovery ID
    fn descendant_sign_digest_recoverable<E, P>(
        &self,
        path: P,
        digest: Hash256Digest,
    ) -> Result<T::RecoverableSignature, Bip32Error>
    where
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.derive_private_path(path)?
            .sign_digest_recoverable(digest)
    }

    /// Derive a descendant, and have it sign a message
    fn descendant_sign_with_hash<D, E, P>(
        &self,
        path: P,
        message: &[u8],
    ) -> Result<T::Signature, Bip32Error>
    where
        D: MarkedDigest<Hash256Digest>,
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.descendant_sign_digest(path, D::digest_marked(message))
    }

    /// Derive a descendant, and have it sign a message and produce a recovery ID
    fn descendant_sign_recoverable_with_hash<D, E, P>(
        &self,
        path: P,
        message: &[u8],
    ) -> Result<T::RecoverableSignature, Bip32Error>
    where
        D: MarkedDigest<Hash256Digest>,
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.descendant_sign_digest_recoverable(path, D::digest_marked(message))
    }

    /// Derive a descendant, and have it produce a signature on `sha2(sha2(message))`
    fn descendant_sign<E, P>(&self, path: P, message: &[u8]) -> Result<T::Signature, Bip32Error>
    where
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.descendant_sign_with_hash::<Hash256, E, P>(path, message)
    }

    /// Derive a descendant, and have it produce a recoverable signature on `sha2(sha2(message))`
    fn descendant_sign_recoverable<E, P>(
        &self,
        path: P,
        message: &[u8],
    ) -> Result<T::RecoverableSignature, Bip32Error>
    where
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.descendant_sign_recoverable_with_hash::<Hash256, E, P>(path, message)
    }
}

/// Shortcuts for deriving and signing.
///
/// This trait is implemented on all types that impl `DerivePublicChild` and `VerifyingKey`
pub trait XVerifying<'a, T: 'a + Secp256k1Backend>:
    DerivePublicChild<'a, T> + VerifyingKey<'a, T>
{
    /// Verify a signature on a digest
    fn descendant_verify_digest<E, P>(
        &self,
        path: P,
        digest: Hash256Digest,
        sig: &T::Signature,
    ) -> Result<(), Bip32Error>
    where
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.derive_public_path(path)?.verify_digest(digest, sig)
    }

    /// Verify a recoverable signature on a digest.
    fn descendant_verify_digest_recoverable<E, P>(
        &self,
        path: P,
        digest: Hash256Digest,
        sig: &T::RecoverableSignature,
    ) -> Result<(), Bip32Error>
    where
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.descendant_verify_digest(path, digest, &sig.without_recovery())
    }

    /// Verify a signature on a message
    fn descendant_verify_with_hash<D, E, P>(
        &self,
        path: P,
        message: &[u8],
        sig: &T::Signature,
    ) -> Result<(), Bip32Error>
    where
        D: MarkedDigest<Hash256Digest>,
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.descendant_verify_digest(path, D::digest_marked(message), sig)
    }

    /// Verify a recoverable signature on a message.
    fn descendant_verify_recoverable_with_hash<D, E, P>(
        &self,
        path: P,
        message: &[u8],
        sig: &T::RecoverableSignature,
    ) -> Result<(), Bip32Error>
    where
        D: MarkedDigest<Hash256Digest>,
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.descendant_verify_digest(path, D::digest_marked(message), &sig.without_recovery())
    }

    /// Produce a signature on `sha2(sha2(message))`
    fn descendant_verify<E, P>(
        &self,
        path: P,
        message: &[u8],
        sig: &T::Signature,
    ) -> Result<(), Bip32Error>
    where
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.descendant_verify_with_hash::<Hash256, E, P>(path, message, sig)
    }

    /// Produce a recoverable signature on `sha2(sha2(message))`
    fn descendant_verify_recoverable<E, P>(
        &self,
        path: P,
        message: &[u8],
        sig: &T::RecoverableSignature,
    ) -> Result<(), Bip32Error>
    where
        E: Into<Bip32Error>,
        P: TryInto<DerivationPath, Error = E>,
    {
        self.descendant_verify_recoverable_with_hash::<Hash256, E, P>(path, message, sig)
    }
}

impl<'a, T, K> XSigning<'a, T> for K
where
    T: 'a + Secp256k1Backend,
    K: DerivePrivateChild<'a, T> + SigningKey<'a, T>,
{
}

impl<'a, T, K> XVerifying<'a, T> for K
where
    T: 'a + Secp256k1Backend,
    K: DerivePublicChild<'a, T> + VerifyingKey<'a, T>,
{
}

/// Comparison operations on keys based on their derivations
pub trait DerivedKey {
    /// The underlying key type
    type Key;

    /// instantiate a new derived key from the key and a derivation
    fn new(k: Self::Key, derivation: KeyDerivation) -> Self;

    /// Return this key's derivation
    fn derivation(&self) -> &KeyDerivation;

    /// `true` if the keys share a root fingerprint, `false` otherwise. Note that on key
    /// fingerprints, which may collide accidentally, or be intentionally collided.
    fn same_root<K: DerivedKey>(&self, other: &K) -> bool {
        self.derivation().same_root(&other.derivation())
    }

    /// `true` if this key is a possible ancestor of the argument, `false` otherwise.
    ///
    /// Warning: this check is cheap, but imprecise. It simply compares the root fingerprints
    /// (which may collide) and checks that `self.path` is a prefix of `other.path`. This may be
    /// deliberately foold by an attacker. For a precise check, use
    /// `GenericDerivedXPriv::is_private_ancestor_of()` or
    /// `GenericDerivedXPub::is_public_ancestor_of()`
    fn is_possible_ancestor_of<K: DerivedKey>(&self, other: &K) -> bool {
        self.derivation()
            .is_possible_ancestor_of(&other.derivation())
    }

    /// Returns the path to the descendant, or `None` if the argument is [definitely not a
    /// descendant.
    ///
    /// This is useful for determining the path to reach some descendant from some ancestor.
    fn path_to_descendant<K: DerivedKey>(&self, other: &K) -> Option<DerivationPath> {
        self.derivation().path_to_descendant(&other.derivation())
    }
}
