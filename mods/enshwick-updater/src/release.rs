//! GitHub releases: their tags, which one is newer, and the files a release
//! carries for a mod.

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

/// The release a mod updates to: its tag and version and where its archive
/// and signature download from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pick {
    /// The release's tag.
    pub tag: String,
    /// The version the tag names.
    pub version: Version,
    /// The archive's file name, `<component>.zip`.
    pub archive: String,
    /// Where the archive downloads from.
    pub archive_url: String,
    /// Where its minisign signature downloads from.
    pub signature_url: String,
}

/// The version a tag names for `component`: `<component>-v<semver>`; `None`
/// for any other tag, such as another component's.
///
/// # Examples
///
/// ```
/// use enshwick_updater::release::version_of;
///
/// let version = version_of("enshwick-safes-v0.2.0", "enshwick-safes").unwrap();
/// assert_eq!(version.to_string(), "0.2.0");
/// assert_eq!(version_of("enshwick-safes-v0.2.0", "enshwick"), None);
/// ```
#[must_use]
pub fn version_of(tag: &str, component: &str) -> Option<Version> {
    let version = tag.strip_prefix(component)?.strip_prefix("-v")?;
    Version::parse(version).ok()
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

/// The newest of `releases` for `component` above `installed` that carries
/// the mod's archive and signature; prereleases count only with
/// `prereleases`, drafts never.
#[must_use]
pub fn newest(
    releases: &[Release],
    repository: &str,
    component: &str,
    installed: &Version,
    prereleases: bool,
) -> Option<Pick> {
    releases
        .iter()
        .filter(|release| !release.draft)
        .filter_map(|release| Some((release, version_of(&release.tag_name, component)?)))
        .filter(|(release, version)| prereleases || (!release.prerelease && version.pre.is_empty()))
        .filter(|(_, version)| version > installed)
        .filter_map(|(release, _)| pick(release, repository, component))
        .max_by(|a, b| a.version.cmp(&b.version))
}

/// The release of exactly `version` for `component` among `releases`, with
/// its archive and signature; prereleases count, drafts do not.
#[must_use]
pub fn exact(
    releases: &[Release],
    repository: &str,
    component: &str,
    version: &Version,
) -> Option<Pick> {
    releases
        .iter()
        .filter(|release| !release.draft)
        .filter(|release| version_of(&release.tag_name, component).as_ref() == Some(version))
        .find_map(|release| pick(release, repository, component))
}

/// `release` as a pick for `component`, when it carries `<component>.zip`
/// and `<component>.zip.minisig` downloaded from the repository's own
/// release page.
fn pick(release: &Release, repository: &str, component: &str) -> Option<Pick> {
    let version = version_of(&release.tag_name, component)?;
    let archive = format!("{component}.zip");
    let signature = format!("{archive}.minisig");
    let prefix = format!(
        "https://github.com/{repository}/releases/download/{}/",
        release.tag_name
    );
    let url = |name: &str| {
        release
            .assets
            .iter()
            .find(|asset| asset.name == name)
            .map(|asset| asset.browser_download_url.clone())
            .filter(|url| url.strip_prefix(&prefix) == Some(name))
    };
    Some(Pick {
        tag: release.tag_name.clone(),
        version,
        archive_url: url(&archive)?,
        signature_url: url(&signature)?,
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
        releases
            .iter()
            .any(|release| version_of(&release.tag_name, component).is_some())
    })
}

/// The API address of `repository`'s release list.
#[must_use]
pub fn list_url(repository: &str) -> String {
    format!("https://api.github.com/repos/{repository}/releases?per_page=100")
}

/// The API address of `repository`'s release tagged `tag`.
#[must_use]
pub fn tag_url(repository: &str, tag: &str) -> String {
    format!("https://api.github.com/repos/{repository}/releases/tags/{tag}")
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
        assert_eq!(
            version_of("enshwick-safes-v0.2.0", "enshwick-safes"),
            Some(v("0.2.0"))
        );
        assert_eq!(
            version_of("enshwick-safes-v1.0.0-beta.1", "enshwick-safes"),
            Some(v("1.0.0-beta.1"))
        );
        assert_eq!(version_of("enshwick-safes-v0.2.0", "enshwick"), None);
        assert_eq!(
            version_of("enshwick-safes-extra-v0.2.0", "enshwick-safes"),
            None
        );
        assert_eq!(version_of("enshwick-safes-v0.2", "enshwick-safes"), None);
        assert_eq!(version_of("v0.2.0", "enshwick-safes"), None);
    }

    #[test]
    fn the_newest_release_above_the_installed_version_is_picked() {
        let releases = vec![
            full("enshwick-safes-v0.1.0", "enshwick-safes"),
            full("enshwick-safes-v0.3.0", "enshwick-safes"),
            full("enshwick-safes-v0.2.0", "enshwick-safes"),
            full("enshwick-loader-v9.0.0", "enshwick-loader"),
        ];
        let pick = newest(&releases, REPO, "enshwick-safes", &v("0.1.0"), false).unwrap();
        assert_eq!(pick.tag, "enshwick-safes-v0.3.0");
        assert_eq!(pick.archive, "enshwick-safes.zip");
        assert!(
            pick.signature_url
                .ends_with("/enshwick-safes-v0.3.0/enshwick-safes.zip.minisig")
        );
        assert_eq!(
            newest(&releases, REPO, "enshwick-safes", &v("0.3.0"), false),
            None
        );
        assert_eq!(
            newest(&releases, REPO, "enshwick-other", &v("0.0.1"), false),
            None
        );
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
        let stable = newest(&releases, REPO, "enshwick-safes", &v("0.1.0"), false).unwrap();
        assert_eq!(stable.version, v("0.2.0"));
        let any = newest(&releases, REPO, "enshwick-safes", &v("0.1.0"), true).unwrap();
        assert_eq!(any.version, v("0.4.0"));
    }

    #[test]
    fn files_from_elsewhere_than_the_release_page_are_refused() {
        let mut moved = full("enshwick-safes-v0.2.0", "enshwick-safes");
        moved.assets[0].browser_download_url = "https://example.com/enshwick-safes.zip".into();
        assert_eq!(
            newest(&[moved], REPO, "enshwick-safes", &v("0.1.0"), false),
            None
        );
    }

    #[test]
    fn an_exact_version_is_found_even_below_the_installed_one() {
        let releases = vec![
            full("enshwick-safes-v0.1.0", "enshwick-safes"),
            full("enshwick-safes-v0.2.0", "enshwick-safes"),
        ];
        let pick = exact(&releases, REPO, "enshwick-safes", &v("0.1.0")).unwrap();
        assert_eq!(pick.tag, "enshwick-safes-v0.1.0");
        assert_eq!(exact(&releases, REPO, "enshwick-safes", &v("0.9.0")), None);
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
