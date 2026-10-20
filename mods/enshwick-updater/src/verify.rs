//! Minisign signatures: an archive is installed only when a trusted key
//! signed it for the release it came from.

use minisign_verify::{PublicKey, Signature};

/// Enshwick's own release key, which signs every archive the repository's
/// release workflow uploads.
pub const ENSHWICK_KEY: &str = include_str!("../keys/enshwick.pub");

/// Why [`check`] refuses an archive none of its keys signed.
pub const UNSIGNED: &str = "no trusted key signed it";

/// A public key from a minisign `.pub` file or its base64 line alone.
///
/// # Errors
///
/// A message when the text is not a minisign public key.
///
/// # Examples
///
/// ```
/// use enshwick_updater::verify::{ENSHWICK_KEY, key};
///
/// assert!(key(ENSHWICK_KEY).is_ok());
/// assert!(key("RWnot-a-key").is_err());
/// ```
pub fn key(text: &str) -> Result<PublicKey, String> {
    let text = text.trim();
    let parsed = if text.contains('\n') {
        PublicKey::decode(text)
    } else {
        PublicKey::from_base64(text)
    };
    parsed.map_err(|e| format!("not a minisign public key: {e}"))
}

/// Checks that one of `keys` signed `archive` with `signature` (the text of
/// its `.minisig`), and that the signature's trusted comment names both the
/// release `tag` and the archive's file name `file`, so a signed archive
/// cannot pass for another release.
///
/// # Errors
///
/// Why the archive is refused.
pub fn check(
    keys: &[PublicKey],
    archive: &[u8],
    signature: &str,
    tag: &str,
    file: &str,
) -> Result<(), String> {
    let signature =
        Signature::decode(signature).map_err(|e| format!("unreadable signature: {e}"))?;
    let signed = keys
        .iter()
        .any(|key| key.verify(archive, &signature, false).is_ok());
    if !signed {
        return Err(UNSIGNED.to_owned());
    }
    let words: Vec<&str> = signature.trusted_comment().split_whitespace().collect();
    if words.contains(&tag) && words.contains(&file) {
        Ok(())
    } else {
        Err(format!(
            "it was signed for \"{}\", not for {tag} {file}",
            signature.trusted_comment()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A throwaway test key and an archive it signed with the trusted
    /// comment `enshwick-safes-v0.2.0 enshwick-safes.zip`.
    const TEST_KEY: &str = "RWSVvUJNE6Z0IzsG2I1G/+JlpjWYHLgP8QVhvMlSWCg0AShBnBmVIt4r";
    const ARCHIVE: &[u8] = b"test";
    const SIGNATURE: &str = include_str!("../testdata/signed.minisig");

    fn keys() -> Vec<PublicKey> {
        vec![key(TEST_KEY).unwrap()]
    }

    #[test]
    fn a_signed_archive_for_its_release_passes() {
        assert_eq!(
            check(
                &keys(),
                ARCHIVE,
                SIGNATURE,
                "enshwick-safes-v0.2.0",
                "enshwick-safes.zip"
            ),
            Ok(())
        );
    }

    #[test]
    fn a_changed_archive_or_another_key_fails() {
        let tag = "enshwick-safes-v0.2.0";
        assert!(check(&keys(), b"tesT", SIGNATURE, tag, "enshwick-safes.zip").is_err());
        let enshwick = vec![key(ENSHWICK_KEY).unwrap()];
        assert!(check(&enshwick, ARCHIVE, SIGNATURE, tag, "enshwick-safes.zip").is_err());
        assert!(check(&[], ARCHIVE, SIGNATURE, tag, "enshwick-safes.zip").is_err());
        assert!(check(&keys(), ARCHIVE, "garbage", tag, "enshwick-safes.zip").is_err());
    }

    #[test]
    fn a_signature_for_another_release_or_file_fails() {
        let file = "enshwick-safes.zip";
        assert!(check(&keys(), ARCHIVE, SIGNATURE, "enshwick-safes-v0.1.0", file).is_err());
        assert!(
            check(
                &keys(),
                ARCHIVE,
                SIGNATURE,
                "enshwick-safes-v0.2.0",
                "enshwick-loader.zip"
            )
            .is_err()
        );
    }

    #[test]
    fn keys_read_from_a_pub_file_or_its_line() {
        assert!(key(ENSHWICK_KEY).is_ok());
        assert!(key(&format!("  {TEST_KEY}\n")).is_ok());
        assert!(key("").is_err());
    }
}
