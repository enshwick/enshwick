//! `url(...)` sources: the JSON document at an `https://` address that lists
//! a mod's versions, which one is newer, and where each downloads from.
//!
//! ```json
//! {
//!   "versions": [
//!     {
//!       "version": "1.2.0",
//!       "url": "https://example.com/my-mod/my-mod-1.2.0.zip",
//!       "signature": "https://example.com/my-mod/my-mod-1.2.0.zip.minisig"
//!     }
//!   ]
//! }
//! ```
//!
//! Each version is SemVer; one with a prerelease part counts only with the
//! `prereleases` setting. `url` is an `https://` address of a `.zip` archive
//! or the mod's `.dll`; `signature`, needed only by a mod that names author
//! keys, is an `https://` address too. An entry that breaks a rule is
//! skipped, other fields are ignored, and the order is free. The file is
//! installed like a GitHub release's: a minisign signature's trusted comment
//! names `<id>-v<version>`, `id` being the mod id, and the file's name.

use semver::Version;

use crate::release::Pick;

/// A `url(...)` source's document.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Document {
    /// The versions it offers, in any order.
    pub versions: Vec<Entry>,
}

/// One version a document offers.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Entry {
    /// The SemVer version, such as `1.2.0`.
    pub version: String,
    /// Where its `.zip` archive or `.dll` downloads from.
    pub url: String,
    /// Where the file's minisign signature downloads from; empty for none.
    #[serde(default)]
    pub signature: String,
}

/// The document in `json`.
///
/// # Errors
///
/// A message when the text is not such a document.
///
/// # Examples
///
/// ```
/// use enshwick_updater::document::parse;
///
/// let json = br#"{"versions":[{"version":"1.2.0","url":"https://example.com/a.zip","signature":"https://example.com/a.zip.minisig"}]}"#;
/// assert_eq!(parse(json).unwrap().versions[0].version, "1.2.0");
/// assert!(parse(b"[]").is_err());
/// ```
pub fn parse(json: &[u8]) -> Result<Document, String> {
    serde_json::from_slice(json).map_err(|e| format!("not an update document: {e}"))
}

/// Whether `url` is an `https://` address with a host.
///
/// # Examples
///
/// ```
/// use enshwick_updater::document::is_https;
///
/// assert!(is_https("https://example.com/versions.json"));
/// assert!(!is_https("http://example.com/versions.json"));
/// assert!(!is_https("https://"));
/// ```
#[must_use]
pub fn is_https(url: &str) -> bool {
    url.strip_prefix("https://")
        .is_some_and(|rest| !rest.is_empty() && !rest.starts_with('/'))
}

/// The file's name: the last segment of `url`'s path, when `url` is
/// `https://` and the name ends in `.zip` or `.dll`.
///
/// # Examples
///
/// ```
/// use enshwick_updater::document::archive_name;
///
/// assert_eq!(archive_name("https://example.com/a/my-mod.zip?x=1"), Some("my-mod.zip"));
/// assert_eq!(archive_name("https://example.com/my_mod.dll"), Some("my_mod.dll"));
/// assert_eq!(archive_name("https://example.com/my-mod.7z"), None);
/// ```
#[must_use]
pub fn archive_name(url: &str) -> Option<&str> {
    if !is_https(url) {
        return None;
    }
    let path = url.split(['?', '#']).next()?;
    let (_, name) = path.strip_prefix("https://")?.rsplit_once('/')?;
    let stem = name
        .len()
        .checked_sub(".zip".len())
        .filter(|&stem| stem > 0)?;
    name.get(stem..)
        .is_some_and(|ext| ext.eq_ignore_ascii_case(".zip") || ext.eq_ignore_ascii_case(".dll"))
        .then_some(name)
}

/// The tag a signature's trusted comment names for version `version` of the
/// mod `id`: `<id>-v<version>`.
///
/// # Examples
///
/// ```
/// use enshwick_updater::document::tag;
///
/// assert_eq!(tag("my-mod", &semver::Version::new(1, 2, 0)), "my-mod-v1.2.0");
/// ```
#[must_use]
pub fn tag(id: &str, version: &Version) -> String {
    format!("{id}-v{version}")
}

/// The newest version in `document` above `installed`, with a signature
/// when `signed`; prereleases count only with `prereleases`.
#[must_use]
pub fn newest(
    document: &Document,
    id: &str,
    installed: &Version,
    prereleases: bool,
    signed: bool,
) -> Option<Pick> {
    document
        .versions
        .iter()
        .filter_map(|entry| pick(entry, id, signed))
        .filter(|pick| prereleases || pick.version.pre.is_empty())
        .filter(|pick| pick.version > *installed)
        .max_by(|a, b| a.version.cmp(&b.version))
}

/// Exactly `version` in `document`, prerelease or not, with a signature
/// when `signed`.
#[must_use]
pub fn exact(document: &Document, id: &str, version: &Version, signed: bool) -> Option<Pick> {
    document
        .versions
        .iter()
        .filter_map(|entry| pick(entry, id, signed))
        .find(|pick| pick.version == *version)
}

/// `entry` as a pick for the mod `id`, when it keeps every rule and has a
/// signature when `signed`.
fn pick(entry: &Entry, id: &str, signed: bool) -> Option<Pick> {
    let version = Version::parse(&entry.version).ok()?;
    let archive = archive_name(&entry.url)?.to_owned();
    let signature_url = match entry.signature.as_str() {
        "" if !signed => None,
        url if is_https(url) => Some(url.to_owned()),
        _ => return None,
    };
    Some(Pick {
        tag: tag(id, &version),
        version,
        archive,
        archive_url: entry.url.clone(),
        signature_url,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(version: &str, url: &str, signature: &str) -> Entry {
        Entry {
            version: version.to_owned(),
            url: url.to_owned(),
            signature: signature.to_owned(),
        }
    }

    fn good(version: &str) -> Entry {
        let url = format!("https://example.com/my-mod-{version}.zip");
        let signature = format!("{url}.minisig");
        entry(version, &url, &signature)
    }

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn the_documented_format_parses() {
        let json = br#"{
            "versions": [
                {
                    "version": "1.2.0",
                    "url": "https://example.com/my-mod/my-mod-1.2.0.zip",
                    "signature": "https://example.com/my-mod/my-mod-1.2.0.zip.minisig",
                    "notes": "ignored"
                }
            ],
            "name": "ignored too"
        }"#;
        let document = parse(json).unwrap();
        assert_eq!(document.versions.len(), 1);
        let pick = newest(&document, "my-mod", &v("1.0.0"), false, true).unwrap();
        assert_eq!(pick.tag, "my-mod-v1.2.0");
        assert_eq!(pick.archive, "my-mod-1.2.0.zip");
        assert_eq!(
            pick.signature_url.as_deref(),
            Some("https://example.com/my-mod/my-mod-1.2.0.zip.minisig")
        );
        assert!(parse(br#"{"versions":[{"version":"1.0.0"}]}"#).is_err());
        let unsigned =
            br#"{"versions":[{"version":"1.3.0","url":"https://example.com/my_mod.dll"}]}"#;
        let unsigned = parse(unsigned).unwrap();
        let pick = newest(&unsigned, "my-mod", &v("1.0.0"), false, false).unwrap();
        assert_eq!(pick.archive, "my_mod.dll");
        assert_eq!(pick.signature_url, None);
        assert_eq!(
            newest(&unsigned, "my-mod", &v("1.0.0"), false, true),
            None,
            "a mod with a key needs the signature"
        );
        assert!(parse(b"<html>").is_err());
        assert!(parse(b"{}").is_err());
    }

    #[test]
    fn the_newest_version_above_the_installed_one_is_picked() {
        let document = Document {
            versions: vec![good("0.1.0"), good("0.3.0"), good("0.2.0")],
        };
        let pick = newest(&document, "my-mod", &v("0.1.0"), false, true).unwrap();
        assert_eq!(pick.version, v("0.3.0"));
        assert_eq!(newest(&document, "my-mod", &v("0.3.0"), false, true), None);
    }

    #[test]
    fn prereleases_count_only_when_asked_for() {
        let document = Document {
            versions: vec![good("0.2.0"), good("0.3.0-rc.1")],
        };
        let stable = newest(&document, "my-mod", &v("0.1.0"), false, true).unwrap();
        assert_eq!(stable.version, v("0.2.0"));
        let any = newest(&document, "my-mod", &v("0.1.0"), true, true).unwrap();
        assert_eq!(any.version, v("0.3.0-rc.1"));
    }

    #[test]
    fn an_exact_version_is_found_even_below_the_installed_one() {
        let document = Document {
            versions: vec![good("0.1.0"), good("0.2.0-beta.1")],
        };
        let pick = exact(&document, "my-mod", &v("0.1.0"), true).unwrap();
        assert_eq!(pick.tag, "my-mod-v0.1.0");
        assert!(exact(&document, "my-mod", &v("0.2.0-beta.1"), true).is_some());
        assert_eq!(exact(&document, "my-mod", &v("0.9.0"), true), None);
    }

    #[test]
    fn entries_that_are_not_https_or_not_archives_are_skipped() {
        let document = Document {
            versions: vec![
                good("0.2.0"),
                entry(
                    "0.5.0",
                    "http://example.com/my-mod.zip",
                    "https://example.com/my-mod.zip.minisig",
                ),
                entry(
                    "0.4.0",
                    "https://example.com/my-mod.zip",
                    "http://example.com/my-mod.zip.minisig",
                ),
                entry(
                    "0.3.0",
                    "https://example.com/my-mod.7z",
                    "https://example.com/my-mod.7z.minisig",
                ),
                entry(
                    "one",
                    "https://example.com/my-mod.zip",
                    "https://example.com/my-mod.zip.minisig",
                ),
            ],
        };
        let pick = newest(&document, "my-mod", &v("0.1.0"), true, true).unwrap();
        assert_eq!(pick.version, v("0.2.0"));
        assert_eq!(exact(&document, "my-mod", &v("0.5.0"), true), None);
        assert_eq!(
            exact(&document, "my-mod", &v("0.4.0"), false),
            None,
            "a signature given must be https:// even when none is needed"
        );
    }

    #[test]
    fn only_https_addresses_with_a_host_pass() {
        assert!(is_https("https://example.com"));
        assert!(!is_https("HTTP://example.com"));
        assert!(!is_https("ftp://example.com/a.zip"));
        assert!(!is_https("https:///a.zip"));
        assert_eq!(archive_name("https://example.com/.zip"), None);
        assert_eq!(archive_name("https://example.com/a.zip#x"), Some("a.zip"));
        assert_eq!(archive_name("http://example.com/a.zip"), None);
    }

    #[test]
    fn a_pick_is_verified_for_the_mod_id_and_the_archive_name() {
        // Signed by the test key in `verify` for
        // `enshwick-safes-v0.2.0 enshwick-safes.zip`.
        let signature = include_str!("../testdata/signed.minisig");
        let keys = [
            crate::verify::key("RWSVvUJNE6Z0IzsG2I1G/+JlpjWYHLgP8QVhvMlSWCg0AShBnBmVIt4r").unwrap(),
        ];
        let document = Document {
            versions: vec![entry(
                "0.2.0",
                "https://example.com/downloads/enshwick-safes.zip",
                "https://example.com/downloads/enshwick-safes.zip.minisig",
            )],
        };
        let check = |id: &str| {
            let pick = exact(&document, id, &v("0.2.0"), true).unwrap();
            crate::verify::check(&keys, b"test", signature, &pick.tag, &pick.archive)
        };
        assert_eq!(check("enshwick-safes"), Ok(()));
        assert!(check("safes").is_err());
    }
}
