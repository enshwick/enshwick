//! The release signature of Enshwick's own files, which marks them as ones
//! the project released.
//!
//! Release CI signs the stub and the loader with the project's Ed25519 key,
//! and the loader checks the signature of each at start. A signature is the
//! raw 64 bytes of the file's Ed25519 signature, in a file beside it named
//! with [`EXTENSION`] appended.

use ed25519_dalek::{Signature, Signer as _, SigningKey, VerifyingKey};

/// The public half of the release key: a file whose signature it checks is
/// one the project released.
pub const PUBLIC_KEY: [u8; 32] = [
    0x6a, 0xd3, 0xbd, 0xce, 0x09, 0xea, 0xa8, 0xaf, 0xac, 0x3b, 0xd6, 0x8c, 0xcc, 0xb6, 0x55, 0xc7,
    0x2e, 0xfc, 0x54, 0xbf, 0xa5, 0x1c, 0xf8, 0x1b, 0x9b, 0x4d, 0x96, 0xac, 0xf2, 0x7b, 0xc0, 0xe6,
];

/// The extension appended to a file's name for its signature:
/// `enshwick.dll.sig` beside `enshwick.dll`.
pub const EXTENSION: &str = "sig";

/// The length of a signature file.
pub const LEN: usize = Signature::BYTE_SIZE;

/// Why a file's signature does not hold.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CheckSignatureError {
    /// The signature file is not [`LEN`] bytes long.
    #[error("a signature is {LEN} bytes, this one {0}")]
    Length(usize),
    /// The signature is not the release key's for these bytes.
    #[error("the signature is not the release key's for this file")]
    Mismatch,
}

/// A signing key that is not the release key the loader checks against.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the signing key is not the release key the loader checks against")]
#[non_exhaustive]
pub struct SignFileError;

/// Signs `file` with the release key's 32-byte secret `seed`, for release
/// CI; a seed of any other key is refused, so a release never ships a
/// signature the loader rejects.
///
/// # Errors
///
/// [`SignFileError`] when `seed` is not the release key's.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::signature::sign;
///
/// assert!(sign(&[7; 32], b"enshwick.dll").is_err());
/// ```
pub fn sign(seed: &[u8; 32], file: &[u8]) -> Result<[u8; LEN], SignFileError> {
    sign_with(&PUBLIC_KEY, seed, file)
}

/// Checks that `signature` is the release key's for `file`: whether the file
/// is one the project released.
///
/// # Errors
///
/// [`CheckSignatureError`] when the signature has the wrong length or does
/// not hold.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::signature::{CheckSignatureError, verify};
///
/// assert_eq!(verify(b"enshwick.dll", &[0; 3]), Err(CheckSignatureError::Length(3)));
/// ```
pub fn verify(file: &[u8], signature: &[u8]) -> Result<(), CheckSignatureError> {
    verify_with(&PUBLIC_KEY, file, signature)
}

fn sign_with(public: &[u8; 32], seed: &[u8; 32], file: &[u8]) -> Result<[u8; LEN], SignFileError> {
    let key = SigningKey::from_bytes(seed);
    if key.verifying_key().to_bytes() != *public {
        return Err(SignFileError);
    }
    Ok(key.sign(file).to_bytes())
}

fn verify_with(
    public: &[u8; 32],
    file: &[u8],
    signature: &[u8],
) -> Result<(), CheckSignatureError> {
    let signature = Signature::from_slice(signature)
        .map_err(|_| CheckSignatureError::Length(signature.len()))?;
    VerifyingKey::from_bytes(public)
        .and_then(|key| key.verify_strict(file, &signature))
        .map_err(|_| CheckSignatureError::Mismatch)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; 32] = [42; 32];

    fn public() -> [u8; 32] {
        SigningKey::from_bytes(&SEED).verifying_key().to_bytes()
    }

    #[test]
    fn the_release_key_is_a_valid_point_that_is_not_weak() {
        let key = VerifyingKey::from_bytes(&PUBLIC_KEY).unwrap();
        assert!(!key.is_weak());
    }

    #[test]
    fn a_file_signed_with_the_key_checks_against_it() {
        let signature = sign_with(&public(), &SEED, b"loader bytes").unwrap();
        assert_eq!(verify_with(&public(), b"loader bytes", &signature), Ok(()));
    }

    #[test]
    fn a_changed_file_or_signature_does_not_check() {
        let mut signature = sign_with(&public(), &SEED, b"loader bytes").unwrap();
        assert_eq!(
            verify_with(&public(), b"loader byteS", &signature),
            Err(CheckSignatureError::Mismatch)
        );
        signature[10] ^= 1;
        assert_eq!(
            verify_with(&public(), b"loader bytes", &signature),
            Err(CheckSignatureError::Mismatch)
        );
    }

    #[test]
    fn a_signature_of_the_wrong_length_names_its_length() {
        assert_eq!(
            verify_with(&public(), b"loader bytes", &[0; 63]),
            Err(CheckSignatureError::Length(63))
        );
    }

    #[test]
    fn a_seed_of_another_key_signs_nothing() {
        assert_eq!(sign_with(&public(), &[1; 32], b"file"), Err(SignFileError));
    }

    #[test]
    fn another_key_s_signature_does_not_check_against_the_release_key() {
        let signature = sign_with(&public(), &SEED, b"file").unwrap();
        assert_eq!(
            verify(b"file", &signature),
            Err(CheckSignatureError::Mismatch)
        );
    }
}
