//! GitHub releases: the version a tag names, which release is newer, and the
//! file a release carries for a mod.
//!
//! A tag names a component's version as `<component>-v1.2.0`,
//! `<component>-1.2.0`, `<component>@1.2.0` or `<component>@v1.2.0`. A bare
//! `v1.2.0` or `1.2.0` counts too, in a repository whose tags name no other
//! component: a repository of one mod. A release's file for a component is,
//! in this order, `<component>.zip`, its only `.zip`, the one `.zip` whose
//! name holds the component, `<component>.dll`, its only `.dll`, or the one
//! `.dll` whose name holds the component; `-` and `_` count as one in names.
//! Its signature, when the release has one, is the file's name with
//! `.minisig` added.

use semver::Version;

/// One GitHub release, as the releases API lists it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Release {
    /// The tag, such as `enshwick-safes-v0.2.0`.
    pub tag_name: String,
    /// Not published yet.
    #[serde(default)]
    pub draft: bool,
    /// Marked as a prerelease.
    #[serde(default)]
    pub prerelease: bool,
    /// The files attached to it.
    #[serde(default)]
    pub assets: Vec<Asset>,
}

/// A file attached to a release.
#[derive(Clone, Debug, PartialEq, Eq, serde::Deserialize)]
pub struct Asset {
    /// The file name, such as `enshwick-safes.zip`.
    pub name: String,
    /// Where it downloads from.
    pub browser_download_url: String,
}

/// The release a mod updates to: its tag and version and where its file and
/// signature download from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    /// The release's tag.
    pub tag: String,
    /// The version the tag names.
    pub version: Version,
    /// The file's name: a `.zip` archive or the mod's `.dll`.
    pub archive: String,
    /// Where the file downloads from.
    pub archive_url: String,
    /// Where its minisign signature downloads from; `None` for a release
    /// without one.
    pub signature_url: Option<String>,
}

/// The version `text` names, with or without a leading `v`.
fn semver(text: &str) -> Option<Version> {
    Version::parse(text.strip_prefix('v').unwrap_or(text)).ok()
}

/// The component a tag names and its version: `<component>-v1.2.0`,
/// `<component>-1.2.0` or `<component>@1.2.0`; `None` for a bare version or
/// a tag that names none.
///
/// # Examples
///
/// ```
/// use enshwick_updater::release::named;
///
/// assert_eq!(named("my-mod-v1.2.0").map(|(c, _)| c), Some("my-mod"));
/// assert_eq!(named("my-mod@1.2.0").map(|(c, _)| c), Some("my-mod"));
/// assert_eq!(named("v1.2.0"), None);
/// assert_eq!(named("build-tables"), None);
/// ```
#[must_use]
pub fn named(tag: &str) -> Option<(&str, Version)> {
    tag.char_indices()
        .filter(|&(at, c)| at > 0 && (c == '-' || c == '@'))
        .find_map(|(at, c)| Some((&tag[..at], semver(&tag[at + c.len_utf8()..])?)))
}

/// The version a tag names for `component`; with `bare`, a tag of a version
/// alone counts too. `None` for any other tag, such as another component's.
///
/// # Examples
///
/// ```
/// use enshwick_updater::release::version_of;
///
/// let version = version_of("enshwick-safes-v0.2.0", "enshwick-safes", false).unwrap();
/// assert_eq!(version.to_string(), "0.2.0");
/// assert_eq!(version_of("enshwick-safes-v0.2.0", "enshwick", false), None);
/// assert!(version_of("v0.2.0", "my-mod", true).is_some());
/// assert_eq!(version_of("v0.2.0", "my-mod", false), None);
/// ```
#[must_use]
pub fn version_of(tag: &str, component: &str, bare: bool) -> Option<Version> {
    if let Some(rest) = tag.strip_prefix(component) {
        let rest = rest.strip_prefix('-').or_else(|| rest.strip_prefix('@'));
        if let Some(version) = rest.and_then(semver) {
            return Some(version);
        }
    }
    if bare { semver(tag) } else { None }
}

/// Whether bare version tags count for `component` in `releases`: no tag
/// among them names another component.
///
/// # Examples
///
/// ```
/// use enshwick_updater::release::{Release, bare};
///
/// let tagged = |tag: &str| Release {
///     tag_name: tag.into(),
///     draft: false,
///     prerelease: false,
///     assets: Vec::new(),
/// };
/// assert!(bare(&[tagged("v1.0.0"), tagged("my-mod-v1.1.0")], "my-mod"));
/// assert!(!bare(&[tagged("v1.0.0"), tagged("other-v1.0.0")], "my-mod"));
/// ```
#[must_use]
pub fn bare(releases: &[Release], component: &str) -> bool {
    releases
        .iter()
        .filter_map(|release| named(&release.tag_name))
        .all(|(named, _)| named == component)
}

/// Releases parsed from the releases API's JSON: a list, or one release.
///
/// # Errors
///
/// A message when the text is not the API's JSON.
pub fn parse(json: &[u8]) -> Result<Vec<Release>, String> {
    let value: serde_json::Value =
        serde_json::from_slice(json).map_err(|e| format!("not JSON: {e}"))?;
    let releases = if value.is_array() {
        serde_json::from_value(value)
    } else {
        serde_json::from_value(value).map(|release| vec![release])
    };
    releases.map_err(|e| format!("not a release list: {e}"))
}

/// The releases of `component` among `releases`, with their versions;
/// drafts never.
fn of<'a>(
    releases: &'a [Release],
    component: &'a str,
) -> impl Iterator<Item = (&'a Release, Version)> {
    let bare = bare(releases, component);
    releases
        .iter()
        .filter(|release| !release.draft)
        .filter_map(move |release| Some((release, version_of(&release.tag_name, component, bare)?)))
}

/// The newest of `releases` for `component` above `installed` that carries
/// a file for it, and its signature when `signed`; prereleases count only
/// with `prereleases`, drafts never.
#[must_use]
pub fn newest(
    releases: &[Release],
    repository: &str,
    component: &str,
    installed: &Version,
    prereleases: bool,
    signed: bool,
) -> Option<Pick> {
    of(releases, component)
        .filter(|(release, version)| prereleases || (!release.prerelease && version.pre.is_empty()))
        .filter(|(_, version)| version > installed)
        .filter_map(|(release, version)| pick(release, version, repository, component, signed))
        .max_by(|a, b| a.version.cmp(&b.version))
}

/// The release of exactly `version` for `component` among `releases`, with
/// its file, and its signature when `signed`; prereleases count, drafts do
/// not.
#[must_use]
pub fn exact(
    releases: &[Release],
    repository: &str,
    component: &str,
    version: &Version,
    signed: bool,
) -> Option<Pick> {
    of(releases, component)
        .filter(|(_, found)| found == version)
        .find_map(|(release, version)| pick(release, version, repository, component, signed))
}

/// `name` folded for comparing: lower case, `_` as `-`.
fn folded(name: &str) -> String {
    name.to_ascii_lowercase().replace('_', "-")
}

/// The file of `release` for `component`, as the module describes.
fn file<'a>(release: &'a Release, component: &str) -> Option<&'a Asset> {
    let component = folded(component);
    let pick = |extension: &str| {
        let files: Vec<&Asset> = release
            .assets
            .iter()
            .filter(|asset| folded(&asset.name).ends_with(extension))
            .collect();
        let exact = files
            .iter()
            .find(|asset| folded(&asset.name) == format!("{component}{extension}"));
        let holding: Vec<&&Asset> = files
            .iter()
            .filter(|asset| folded(&asset.name).contains(&component))
            .collect();
        match (exact, &files[..], &holding[..]) {
            (Some(exact), _, _) => Some(*exact),
            (None, [only], _) => Some(*only),
            (None, _, [only]) => Some(**only),
            _ => None,
        }
    };
    pick(".zip").or_else(|| pick(".dll"))
}

/// `release` as a pick of `version` for `component`, when it carries a file
/// for it, and its signature when `signed`, downloaded from the
/// repository's own release page.
fn pick(
    release: &Release,
    version: Version,
    repository: &str,
    component: &str,
    signed: bool,
) -> Option<Pick> {
    let prefix = format!("https://github.com/{repository}/releases/download/");
    // GitHub escapes the tag in the address, such as `@` as `%40`.
    let url = |name: &str| {
        release
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
            .filter(|url| {
                url.strip_prefix(&prefix)
                    .and_then(|rest| {
                        percent_encoding::percent_decode_str(rest)
                            .decode_utf8()
                            .ok()
                    })
                    .is_some_and(|rest| {
                        rest.strip_prefix(release.tag_name.as_str())
                            .and_then(|rest| rest.strip_prefix('/'))
                            == Some(name)
                    })
            })
    };
    let archive = file(release, component)?.name.clone();
    let signature_url = url(&format!("{archive}.minisig"));
    if signed && signature_url.is_none() {
        return None;
    }
    Some(Pick {
        tag: release.tag_name.clone(),
        version,
        archive_url: url(&archive)?,
        signature_url,
        archive,
    })
}

/// Whether `releases` hold a release of each of `components`: GitHub lists
/// releases newest first, so the pages read so far hold the newest of each.
///
/// # Examples
///
/// ```
/// use enshwick_updater::release::{Release, seen_all};
///
/// let release = Release {
///     tag_name: "enshwick-safes-v0.2.0".into(),
///     draft: false,
///     prerelease: false,
///     assets: Vec::new(),
/// };
/// assert!(seen_all(&[release.clone()], &["enshwick-safes"]));
/// assert!(!seen_all(&[release], &["enshwick-safes", "enshwick-updater"]));
/// ```
#[must_use]
pub fn seen_all(releases: &[Release], components: &[&str]) -> bool {
    components.iter().all(|component| {
        let bare = bare(releases, component);
        releases
            .iter()
            .any(|release| version_of(&release.tag_name, component, bare).is_some())
    })
}

/// The API address of `repository`'s release list.
#[must_use]
pub fn list_url(repository: &str) -> String {
    format!("https://api.github.com/repos/{repository}/releases?per_page=100")
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO: &str = "themaiby/enshwick";

    fn release(tag: &str, prerelease: bool, files: &[&str]) -> Release {
        Release {
            tag_name: tag.to_owned(),
            draft: false,
            prerelease,
            assets: files
                .iter()
                .map(|name| Asset {
                    name: (*name).to_owned(),
                    browser_download_url: format!(
                        "https://github.com/{REPO}/releases/download/{tag}/{name}"
                    ),
                })
                .collect(),
        }
    }

    fn full(tag: &str, component: &str) -> Release {
        release(
            tag,
            false,
            &[
                &format!("{component}.zip"),
                &format!("{component}.zip.minisig"),
            ],
        )
    }

    fn v(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    #[test]
    fn tags_name_versions_of_their_own_component_only() {
        let safes = "enshwick-safes";
        assert_eq!(
            version_of("enshwick-safes-v0.2.0", safes, false),
            Some(v("0.2.0"))
        );
        assert_eq!(
            version_of("enshwick-safes-v1.0.0-beta.1", safes, false),
            Some(v("1.0.0-beta.1"))
        );
        assert_eq!(version_of("enshwick-safes-v0.2.0", "enshwick", false), None);
        assert_eq!(
            version_of("enshwick-safes-extra-v0.2.0", safes, false),
            None
        );
        assert_eq!(version_of("enshwick-safes-v0.2", safes, false), None);
        assert_eq!(version_of("v0.2.0", safes, false), None);
    }

    #[test]
    fn every_tag_form_names_its_version() {
        for tag in [
            "my-mod-v1.2.0",
            "my-mod-1.2.0",
            "my-mod@1.2.0",
            "my-mod@v1.2.0",
        ] {
            assert_eq!(version_of(tag, "my-mod", false), Some(v("1.2.0")), "{tag}");
            assert_eq!(named(tag), Some(("my-mod", v("1.2.0"))), "{tag}");
        }
        for tag in ["v1.2.0", "1.2.0"] {
            assert_eq!(version_of(tag, "my-mod", true), Some(v("1.2.0")), "{tag}");
            assert_eq!(version_of(tag, "my-mod", false), None, "{tag}");
            assert_eq!(named(tag), None, "{tag}");
        }
        assert_eq!(
            named("my-mod-1.2.0-rc.1"),
            Some(("my-mod", v("1.2.0-rc.1")))
        );
        assert_eq!(named("my-mod2-1.0.0"), Some(("my-mod2", v("1.0.0"))));
        assert_eq!(named("nightly"), None);
    }

    #[test]
    fn a_tag_with_an_at_sign_downloads_from_its_escaped_address() {
        let mut at = release("my-mod@1.2.0", false, &["my-mod.zip", "my-mod.zip.minisig"]);
        for asset in &mut at.assets {
            // As GitHub lists it.
            asset.browser_download_url = asset.browser_download_url.replace('@', "%40");
        }
        let pick = newest(&[at], REPO, "my-mod", &v("1.0.0"), false, true).unwrap();
        assert_eq!(pick.tag, "my-mod@1.2.0");
        assert_eq!(
            pick.archive_url,
            format!("https://github.com/{REPO}/releases/download/my-mod%401.2.0/my-mod.zip")
        );
        assert!(pick.signature_url.is_some());
        // Another release's file stays refused, escaped or not.
        let mut elsewhere = release("my-mod@1.3.0", false, &["my-mod.zip"]);
        elsewhere.assets[0].browser_download_url =
            format!("https://github.com/{REPO}/releases/download/my-mod%401.2.0/my-mod.zip");
        assert_eq!(
            newest(&[elsewhere], REPO, "my-mod", &v("1.0.0"), false, false),
            None
        );
    }

    #[test]
    fn bare_versions_count_only_in_a_repository_of_one_mod() {
        let single = vec![
            release("v1.1.0", false, &["my-mod.zip"]),
            release("1.0.0", false, &["my-mod.zip"]),
        ];
        let pick = newest(&single, REPO, "my-mod", &v("1.0.0"), false, false).unwrap();
        assert_eq!(pick.version, v("1.1.0"));
        assert!(seen_all(&single, &["my-mod"]));
        let mono = vec![
            release("v2.0.0", false, &["my-mod.zip"]),
            release("other-v1.0.0", false, &["other.zip"]),
            release("my-mod-v1.1.0", false, &["my-mod.zip"]),
        ];
        let pick = newest(&mono, REPO, "my-mod", &v("1.0.0"), false, false).unwrap();
        assert_eq!(pick.version, v("1.1.0"), "the bare tag is not my-mod's");
    }

    #[test]
    fn the_file_is_the_components_zip_any_single_zip_or_a_dll() {
        let file_of = |files: &[&str]| {
            let release = release("my-mod-v1.0.0", false, files);
            pick(&release, v("1.0.0"), REPO, "my-mod", false).map(|p| p.archive)
        };
        let named = |name: &str| Some(name.to_owned());
        assert_eq!(file_of(&["other.zip", "my-mod.zip"]), named("my-mod.zip"));
        assert_eq!(file_of(&["MyMod-Windows.zip"]), named("MyMod-Windows.zip"));
        assert_eq!(
            file_of(&["my_mod-1.0.0.zip", "sources.zip"]),
            named("my_mod-1.0.0.zip")
        );
        assert_eq!(file_of(&["a.zip", "b.zip"]), None, "which zip is unclear");
        assert_eq!(file_of(&["my_mod.dll", "README.md"]), named("my_mod.dll"));
        assert_eq!(file_of(&["anything.dll"]), named("anything.dll"));
        assert_eq!(file_of(&["my-mod.zip", "my_mod.dll"]), named("my-mod.zip"));
        assert_eq!(file_of(&["README.md"]), None);
    }

    #[test]
    fn a_signed_mod_needs_the_files_signature() {
        let unsigned = release("my-mod-v1.0.0", false, &["my-mod.zip"]);
        assert!(pick(&unsigned, v("1.0.0"), REPO, "my-mod", true).is_none());
        let found = pick(&unsigned, v("1.0.0"), REPO, "my-mod", false).unwrap();
        assert_eq!(found.signature_url, None);
        let signed = release(
            "my-mod-v1.0.0",
            false,
            &["my_mod.dll", "my_mod.dll.minisig"],
        );
        let found = pick(&signed, v("1.0.0"), REPO, "my-mod", true).unwrap();
        assert!(
            found
                .signature_url
                .is_some_and(|url| url.ends_with("/my_mod.dll.minisig"))
        );
    }

    #[test]
    fn the_newest_release_above_the_installed_version_is_picked() {
        let releases = vec![
            full("enshwick-safes-v0.1.0", "enshwick-safes"),
            full("enshwick-safes-v0.3.0", "enshwick-safes"),
            full("enshwick-safes-v0.2.0", "enshwick-safes"),
            full("enshwick-loader-v9.0.0", "enshwick-loader"),
        ];
        let newest =
            |component, installed| newest(&releases, REPO, component, &v(installed), false, true);
        let pick = newest("enshwick-safes", "0.1.0").unwrap();
        assert_eq!(pick.tag, "enshwick-safes-v0.3.0");
        assert_eq!(pick.archive, "enshwick-safes.zip");
        assert!(
            pick.signature_url.is_some_and(
                |url| url.ends_with("/enshwick-safes-v0.3.0/enshwick-safes.zip.minisig")
            )
        );
        assert_eq!(newest("enshwick-safes", "0.3.0"), None);
        assert_eq!(newest("enshwick-other", "0.0.1"), None);
    }

    #[test]
    fn prereleases_drafts_and_releases_without_signatures_are_skipped() {
        let mut draft = full("enshwick-safes-v0.5.0", "enshwick-safes");
        draft.draft = true;
        let releases = vec![
            full("enshwick-safes-v0.2.0", "enshwick-safes"),
            draft,
            release(
                "enshwick-safes-v0.4.0",
                true,
                &["enshwick-safes.zip", "enshwick-safes.zip.minisig"],
            ),
            full("enshwick-safes-v0.3.1-rc.1", "enshwick-safes"),
            release("enshwick-safes-v0.3.0", false, &["enshwick-safes.zip"]),
        ];
        let newest = |prereleases, signed| {
            newest(
                &releases,
                REPO,
                "enshwick-safes",
                &v("0.1.0"),
                prereleases,
                signed,
            )
            .unwrap()
            .version
        };
        assert_eq!(newest(false, true), v("0.2.0"));
        assert_eq!(newest(true, true), v("0.4.0"));
        assert_eq!(
            newest(false, false),
            v("0.3.0"),
            "unsigned counts without a key"
        );
    }

    #[test]
    fn files_from_elsewhere_than_the_release_page_are_refused() {
        let mut moved = full("enshwick-safes-v0.2.0", "enshwick-safes");
        moved.assets[0].browser_download_url = "https://example.com/enshwick-safes.zip".into();
        assert_eq!(
            newest(&[moved], REPO, "enshwick-safes", &v("0.1.0"), false, false),
            None
        );
    }

    #[test]
    fn an_exact_version_is_found_even_below_the_installed_one() {
        let releases = vec![
            full("enshwick-safes-v0.1.0", "enshwick-safes"),
            full("enshwick-safes-v0.2.0", "enshwick-safes"),
        ];
        let pick = exact(&releases, REPO, "enshwick-safes", &v("0.1.0"), true).unwrap();
        assert_eq!(pick.tag, "enshwick-safes-v0.1.0");
        assert_eq!(
            exact(&releases, REPO, "enshwick-safes", &v("0.9.0"), true),
            None
        );
    }

    #[test]
    fn the_api_json_parses_as_a_list_or_one_release() {
        let list = br#"[{"tag_name":"a-v1.0.0","draft":false,"prerelease":false,"assets":[{"name":"a.zip","browser_download_url":"https://github.com/o/r/releases/download/a-v1.0.0/a.zip","size":3}],"body":"x"}]"#;
        let releases = parse(list).unwrap();
        assert_eq!(releases.len(), 1);
        assert_eq!(releases[0].assets[0].name, "a.zip");
        let one = br#"{"tag_name":"a-v1.0.0"}"#;
        assert_eq!(parse(one).unwrap()[0].tag_name, "a-v1.0.0");
        assert!(parse(b"{\"message\":\"Not Found\"}").is_err());
        assert!(parse(b"<html>").is_err());
    }
}
