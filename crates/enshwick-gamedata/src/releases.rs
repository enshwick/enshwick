//! Which Enshwick release is newer than a running one, read from GitHub's
//! answer: the loader and `cargo enshwick` ask the same question, each by
//! the tags it is released under.

use semver::{BuildMetadata, Version};

/// The page of a release, without its tag.
const PAGE: &str = "https://github.com/enshwick/enshwick/releases/tag/";
/// The tag prefixes of a loader release: every release, alpha or not, is
/// `v<version>`, the one GitHub release of that version.
pub const LOADER_TAGS: &[&str] = &["v"];
/// The tag prefixes of a `cargo enshwick` release, which ships in the
/// loader's: release-plz's `cargo-enshwick-v<version>` tag carries no
/// release and no binaries.
pub const TOOL_TAGS: &[&str] = &["v"];
/// What a prerelease asks GitHub: the newest releases, one per version,
/// among which the next alpha always is.
const RELEASES: &str = "/repos/enshwick/enshwick/releases?per_page=20";
/// What a release asks GitHub: its latest release, the newest published
/// stable one however many prereleases came after it. A tag counts only once
/// its release is published: release-plz pushes the tag before the release
/// workflow has built the files.
const LATEST: &str = "/repos/enshwick/enshwick/releases/latest";
/// The most bytes of an answer worth reading; twenty releases with their
/// notes take under it.
pub const LIMIT: usize = 4 << 20;

/// One release as GitHub lists it; the other fields are ignored.
#[derive(serde::Deserialize)]
struct Listed {
    tag_name: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

/// A release newer than the running one.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::releases::{LOADER_TAGS, newer};
///
/// let answer = br#"[{"tag_name":"v1.0.0-alpha.14","prerelease":true}]"#;
/// let running = semver::Version::parse("1.0.0-alpha.13")?;
/// let release = newer(answer, &running, LOADER_TAGS)?.ok_or("none newer")?;
/// assert_eq!(release.version().to_string(), "1.0.0-alpha.14");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    version: Version,
    /// The tag prefix it was published under, one of the prefixes asked for.
    prefix: &'static str,
}

impl Release {
    /// The release's version, to compare with the running one or to show.
    ///
    /// # Examples
    ///
    /// See [`Release`].
    #[must_use]
    pub fn version(&self) -> &Version {
        &self.version
    }

    /// The release's page, built here rather than read from GitHub's answer,
    /// so no address the network chose is shown.
    ///
    /// # Examples
    ///
    /// See [`Release`]; its page is
    /// `https://github.com/enshwick/enshwick/releases/tag/v1.0.0-alpha.14`.
    #[must_use]
    pub fn page(&self) -> String {
        format!("{PAGE}{}{}", self.prefix, self.version)
    }
}

/// The path on `api.github.com` whose answer [`newer`] reads for `running`.
///
/// # Examples
///
/// ```
/// let running = semver::Version::parse("1.0.0-alpha.13")?;
/// assert!(enshwick_gamedata::releases::path(&running).starts_with("/repos/"));
/// # Ok::<(), semver::Error>(())
/// ```
#[must_use]
pub fn path(running: &Version) -> &'static str {
    if running.pre.is_empty() {
        LATEST
    } else {
        RELEASES
    }
}

/// The version a build of `release` states: `release` itself, or for a
/// development build of `commit` one that no release takes.
///
/// A development build marks itself in the build metadata, which no release
/// carries: `1.0.0-alpha.15` becomes `1.0.0-alpha.15+dev.<commit>`. SemVer
/// ignores build metadata in precedence, so the build keeps its release's
/// channel and is offered every newer release. The commit keeps its first
/// eight hex digits; one that is not hex, or empty, states `release`.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::releases::stated;
///
/// let release = semver::Version::parse("1.0.0-alpha.15")?;
/// let dev = stated(&release, Some("5f647f46c0ffee"));
/// assert_eq!(dev.to_string(), "1.0.0-alpha.15+dev.5f647f46");
/// assert_eq!(stated(&release, None), release);
/// # Ok::<(), semver::Error>(())
/// ```
#[must_use]
pub fn stated(release: &Version, commit: Option<&str>) -> Version {
    let Some(commit) = commit_tag(commit) else {
        return release.clone();
    };
    let mut dev = release.clone();
    // A hex commit after `dev.` is a valid identifier: it never falls back.
    dev.build = BuildMetadata::new(&format!("{DEV}{commit}")).unwrap_or_default();
    dev
}

/// What a development build's version puts before its commit, after `+`.
pub const DEV: &str = "dev.";

/// The part of `commit` a development build states, as [`stated`] does.
///
/// It is the first eight hex digits; `None` for none, an empty one or one
/// that is not hex. A `const fn`, so a version can be assembled at compile
/// time.
///
/// # Examples
///
/// ```
/// use enshwick_gamedata::releases::commit_tag;
///
/// assert_eq!(commit_tag(Some("5f647f46c0ffee")), Some("5f647f46"));
/// assert_eq!(commit_tag(Some("abc")), Some("abc"));
/// assert_eq!(commit_tag(Some("main")), None);
/// assert_eq!(commit_tag(None), None);
/// ```
#[must_use]
pub const fn commit_tag(commit: Option<&str>) -> Option<&str> {
    let Some(commit) = commit else {
        return None;
    };
    let bytes = commit.as_bytes();
    if bytes.is_empty() {
        return None;
    }
    let mut at = 0;
    while at < bytes.len() {
        if !bytes[at].is_ascii_hexdigit() {
            return None;
        }
        at += 1;
    }
    if bytes.len() <= 8 {
        Some(commit)
    } else {
        Some(commit.split_at(8).0)
    }
}

/// The line the loader and `cargo enshwick` state their version in, with
/// the ABI version `major`.`minor` beside it.
///
/// # Examples
///
/// ```
/// let version = semver::Version::parse("1.0.0-alpha.15")?;
/// let line = enshwick_gamedata::releases::version_line(&version, 11, 30);
/// assert_eq!(line, "1.0.0-alpha.15 (ABI 11.30)");
/// # Ok::<(), semver::Error>(())
/// ```
#[must_use]
pub fn version_line(version: &Version, major: u16, minor: u16) -> String {
    format!("{version} (ABI {major}.{minor})")
}

/// The newest release in `answer`, GitHub's JSON answer to [`path`], tagged
/// under one of `prefixes` and newer than `running` by SemVer precedence.
///
/// A prerelease counts only while `running` is one: by its version, and in
/// the list of releases an alpha reads also by GitHub's flag. A running
/// release reads GitHub's one latest release. A tag under none of
/// `prefixes` is skipped. One version under two prefixes takes the one
/// listed first.
///
/// # Errors
///
/// When `answer` is no JSON list of releases, or no single release for a
/// running release.
///
/// # Examples
///
/// See [`Release`].
pub fn newer(
    answer: &[u8],
    running: &Version,
    prefixes: &'static [&'static str],
) -> Result<Option<Release>, serde_json::Error> {
    let listed: Vec<Listed> = if running.pre.is_empty() {
        vec![serde_json::from_slice(answer)?]
    } else {
        serde_json::from_slice(answer)?
    };
    Ok(listed
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| {
            let (prefix, version) = prefixes.iter().find_map(|prefix| {
                let version = Version::parse(release.tag_name.strip_prefix(prefix)?).ok()?;
                Some((*prefix, version))
            })?;
            let pre = release.prerelease || !version.pre.is_empty();
            (!pre || !running.pre.is_empty()).then_some(Release { version, prefix })
        })
        .filter(|release| release.version.cmp_precedence(running).is_gt())
        .max_by(|a, b| {
            let rank = |release: &Release| prefixes.iter().position(|p| *p == release.prefix);
            a.version
                .cmp_precedence(&b.version)
                .then_with(|| rank(b).cmp(&rank(a)))
        }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    /// A listing of `(tag, draft, prerelease)`.
    fn flagged(tags: &[(&str, bool, bool)]) -> Vec<u8> {
        let releases: Vec<String> = tags
            .iter()
            .map(|(tag, draft, pre)| {
                format!(r#"{{"tag_name":"{tag}","draft":{draft},"prerelease":{pre},"body":"x"}}"#)
            })
            .collect();
        format!("[{}]", releases.join(",")).into_bytes()
    }

    /// A listing of `(tag, draft)`, marked prerelease as the release workflow
    /// marks it: by a prerelease part in the tag.
    fn listing(tags: &[(&str, bool)]) -> Vec<u8> {
        let flags: Vec<(&str, bool, bool)> = tags
            .iter()
            .map(|(tag, draft)| (*tag, *draft, tag.contains('-')))
            .collect();
        flagged(&flags)
    }

    fn found(list: &[u8], running: &str) -> Option<Version> {
        newer(list, &version(running), LOADER_TAGS)
            .unwrap()
            .map(|release| release.version)
    }

    #[test]
    fn a_newer_alpha_is_found_while_an_alpha_runs() {
        let list = listing(&[("v1.0.0-alpha.13", false), ("v1.0.0-alpha.12", false)]);
        assert_eq!(
            found(&list, "1.0.0-alpha.12"),
            Some(version("1.0.0-alpha.13"))
        );
    }

    #[test]
    fn the_running_release_and_older_ones_are_not_newer() {
        let list = listing(&[("v1.0.0-alpha.12", false), ("v1.0.0-alpha.9", false)]);
        assert_eq!(found(&list, "1.0.0-alpha.12"), None);
    }

    #[test]
    fn the_highest_version_wins_whatever_the_order() {
        let list = listing(&[
            ("v1.0.0-alpha.13", false),
            ("v1.0.0-alpha.20", false),
            ("v1.0.0-alpha.14", false),
        ]);
        assert_eq!(
            found(&list, "1.0.0-alpha.12"),
            Some(version("1.0.0-alpha.20"))
        );
    }

    /// GitHub's answer for the latest release: one release, not a list.
    fn latest(tag: &str, prerelease: bool) -> Vec<u8> {
        format!(r#"{{"tag_name":"{tag}","draft":false,"prerelease":{prerelease},"body":"x"}}"#)
            .into_bytes()
    }

    #[test]
    fn a_release_asks_for_the_latest_release_and_an_alpha_for_the_newest_releases() {
        assert_eq!(path(&version("1.0.0")), LATEST);
        assert_eq!(path(&version("1.0.0-alpha.12")), RELEASES);
    }

    #[test]
    fn a_running_release_counts_no_tag_whose_release_is_not_published() {
        let tags = br#"[{"ref":"refs/tags/v1.0.1","object":{"type":"commit"}}]"#;
        assert!(newer(tags, &version("1.0.0"), LOADER_TAGS).is_err());
        assert_eq!(
            found(&latest("v1.0.1", false), "1.0.0"),
            Some(version("1.0.1"))
        );
    }

    #[test]
    fn a_release_runs_ahead_of_alphas_but_ignores_them() {
        assert_eq!(
            found(&latest("v1.0.1", false), "1.0.0"),
            Some(version("1.0.1"))
        );
        assert_eq!(found(&latest("v1.1.0-alpha.1", false), "1.0.0"), None);
        assert_eq!(found(&latest("v1.1.0", true), "1.0.0"), None);
    }

    #[test]
    fn the_crates_release_plz_tags_carry_no_release_and_are_skipped() {
        for tag in [
            "cargo-enshwick-v1.5.0",
            "enshwick-loader-stub-v1.5.0",
            "enshwick-v3.0.0",
        ] {
            assert_eq!(found(&latest(tag, false), "1.0.0"), None, "{tag}");
        }
        let tool = newer(&latest("v1.0.2", false), &version("1.0.0"), TOOL_TAGS)
            .unwrap()
            .unwrap();
        assert_eq!(tool.page(), format!("{PAGE}v1.0.2"));
    }

    #[test]
    fn a_version_under_two_prefixes_links_the_one_listed_first() {
        const BOTH: &[&str] = &["v", "x-v"];
        let running = version("1.0.0-alpha.1");
        for tags in [
            [("x-v1.1.0", false), ("v1.1.0", false)],
            [("v1.1.0", false), ("x-v1.1.0", false)],
        ] {
            let release = newer(&listing(&tags), &running, BOTH).unwrap().unwrap();
            assert_eq!(release.page(), format!("{PAGE}v1.1.0"));
        }
        let tags = listing(&[("x-v1.2.0", false), ("v1.1.0", false)]);
        let release = newer(&tags, &running, BOTH).unwrap().unwrap();
        assert_eq!(release.page(), format!("{PAGE}x-v1.2.0"));
    }

    #[test]
    fn a_release_github_marks_prerelease_counts_while_an_alpha_runs() {
        let list = flagged(&[("v1.1.0", false, true)]);
        assert_eq!(found(&list, "1.0.0-alpha.12"), Some(version("1.1.0")));
    }

    #[test]
    fn the_release_after_an_alpha_counts_while_the_alpha_runs() {
        let list = listing(&[("v1.0.0", false)]);
        assert_eq!(found(&list, "1.0.0-alpha.12"), Some(version("1.0.0")));
    }

    #[test]
    fn build_metadata_alone_is_not_a_newer_release() {
        let list = listing(&[("v1.0.0-alpha.12+rebuild.1", false)]);
        assert_eq!(found(&list, "1.0.0-alpha.12"), None);
    }

    #[test]
    fn the_first_release_is_found_with_its_page_while_an_alpha_runs() {
        let list = listing(&[("v1.0.0", false), ("enshwick-v1.2.0", false)]);
        let release = newer(&list, &version("1.0.0-alpha.12"), LOADER_TAGS)
            .unwrap()
            .unwrap();
        assert_eq!(release.version, version("1.0.0"));
        assert_eq!(
            release.page(),
            "https://github.com/enshwick/enshwick/releases/tag/v1.0.0"
        );
    }

    #[test]
    fn drafts_and_tags_that_are_no_version_are_skipped() {
        let list = listing(&[
            ("v2.0.0", true),
            ("enshwick-v3.0.0", false),
            ("dev", false),
            ("v1.0.0-alpha.13", false),
        ]);
        assert_eq!(
            found(&list, "1.0.0-alpha.12"),
            Some(version("1.0.0-alpha.13"))
        );
    }

    #[test]
    fn an_answer_that_is_no_list_of_releases_is_an_error() {
        let running = version("1.0.0-alpha.12");
        let rate_limited = br#"{"message":"API rate limit exceeded"}"#;
        assert!(newer(rate_limited, &running, LOADER_TAGS).is_err());
        assert!(newer(b"<html>", &running, LOADER_TAGS).is_err());
        assert_eq!(newer(b"[]", &running, LOADER_TAGS).unwrap(), None);
        let running = version("1.0.0");
        assert!(newer(br#"{"message":"Not Found"}"#, &running, LOADER_TAGS).is_err());
        assert!(newer(&listing(&[("v1.0.1", false)]), &running, LOADER_TAGS).is_err());
    }

    #[test]
    fn the_page_is_built_from_the_tag() {
        let list = listing(&[("v1.0.0-alpha.13", false)]);
        let release = newer(&list, &version("1.0.0-alpha.12"), LOADER_TAGS)
            .unwrap()
            .unwrap();
        assert_eq!(
            release.page(),
            "https://github.com/enshwick/enshwick/releases/tag/v1.0.0-alpha.13"
        );
    }

    #[test]
    fn a_development_build_keeps_its_releases_precedence_and_channel() {
        for (release, dev) in [
            ("1.0.0-alpha.15", "1.0.0-alpha.15+dev.5f647f46"),
            ("1.0.0", "1.0.0+dev.5f647f46"),
        ] {
            let stated = stated(&version(release), Some("5f647f46c0ffee00"));
            assert_eq!(stated, version(dev));
            assert!(stated.cmp_precedence(&version(release)).is_eq(), "{dev}");
            assert_eq!(path(&stated), path(&version(release)), "{dev}");
        }
    }

    #[test]
    fn a_development_build_of_a_stable_release_is_offered_no_prerelease() {
        let running = stated(&version("1.0.0"), Some("5f647f46"));
        let found = newer(&latest("v1.0.1", false), &running, LOADER_TAGS).unwrap();
        assert_eq!(found.map(|r| r.version), Some(version("1.0.1")));
        let found = newer(&latest("v1.1.0-alpha.1", true), &running, LOADER_TAGS).unwrap();
        assert_eq!(found, None);
    }

    #[test]
    fn a_development_build_is_offered_a_hotfix_of_its_release() {
        let list = listing(&[("v1.0.0-alpha.15.1", false)]);
        let running = stated(&version("1.0.0-alpha.15"), Some("5f647f46"));
        let found = newer(&list, &running, LOADER_TAGS).unwrap();
        assert_eq!(found.map(|r| r.version), Some(version("1.0.0-alpha.15.1")));
    }

    #[test]
    fn a_build_without_a_hex_commit_states_its_release() {
        let release = version("1.0.0-alpha.15");
        for commit in [None, Some(""), Some("main"), Some("5f64 7f46")] {
            assert_eq!(stated(&release, commit), release, "{commit:?}");
        }
        assert_eq!(
            stated(&release, Some("abc")).to_string(),
            "1.0.0-alpha.15+dev.abc",
            "a short commit is kept whole"
        );
    }

    #[test]
    fn a_running_development_build_is_offered_the_next_alpha() {
        let list = listing(&[("v1.0.0-alpha.16", false), ("v1.0.0-alpha.15", false)]);
        let running = stated(&version("1.0.0-alpha.15"), Some("5f647f46"));
        let found = newer(&list, &running, LOADER_TAGS).unwrap();
        assert_eq!(found.map(|r| r.version), Some(version("1.0.0-alpha.16")));
    }
}
