//! Which Enshwick release is newer than a running one, read from GitHub's
//! answer: the loader and `cargo enshwick` ask the same question, each by
//! the tags it is released under.

use semver::Version;

/// The page of a release, without its tag.
const PAGE: &str = "https://github.com/enshwick/enshwick/releases/tag/";
/// The tag prefixes of a loader release: an alpha's, and release-plz's for
/// the loader package.
pub const LOADER_TAGS: &[&str] = &["v", "enshwick-loader-v"];
/// The tag prefixes of a `cargo enshwick` release: an alpha's, and
/// release-plz's for its package.
pub const TOOL_TAGS: &[&str] = &["v", "cargo-enshwick-v"];
/// What a prerelease asks GitHub: the newest releases, among which the next
/// alpha always is.
const RELEASES: &str = "/repos/enshwick/enshwick/releases?per_page=20";
/// What a release asks GitHub: every tag, a few hundred bytes each and never
/// paged, so a stable release under either prefix is found however many
/// releases came after it.
const TAGS: &str = "/repos/enshwick/enshwick/git/matching-refs/tags/";
/// The prefix of a tag's name in a Git reference.
const TAG_REF: &str = "refs/tags/";
/// The most bytes of an answer worth reading; twenty releases with their
/// notes, or some ten thousand tags, take under it.
pub const LIMIT: usize = 4 << 20;

/// One Git reference as GitHub lists it; the other fields are ignored.
#[derive(serde::Deserialize)]
struct Ref {
    #[serde(rename = "ref")]
    name: String,
}

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
/// assert_eq!(release.version.to_string(), "1.0.0-alpha.14");
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Debug, PartialEq, Eq)]
pub struct Release {
    /// The release's version.
    pub version: Version,
    /// The tag prefix it was published under, one of the prefixes asked for.
    prefix: &'static str,
}

impl Release {
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
        TAGS
    } else {
        RELEASES
    }
}

/// The newest release in `answer`, GitHub's JSON answer to [`path`], tagged
/// under one of `prefixes` and newer than `running` by SemVer precedence.
///
/// A prerelease counts only while `running` is one: by its version, and in
/// the list of releases an alpha reads also by GitHub's flag. A running
/// release reads tags, so a tag with a release version counts as released.
/// A tag under none of `prefixes` is skipped. One version under two
/// prefixes takes the one other than `v`, release-plz's, which carries the
/// GitHub release.
///
/// # Errors
///
/// When `answer` is no JSON list of releases, or of Git references for a
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
        serde_json::from_slice::<Vec<Ref>>(answer)?
            .into_iter()
            .filter_map(|tag| {
                Some(Listed {
                    tag_name: tag.name.strip_prefix(TAG_REF)?.to_owned(),
                    draft: false,
                    prerelease: false,
                })
            })
            .collect()
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
            a.version
                .cmp_precedence(&b.version)
                .then_with(|| (a.prefix != "v").cmp(&(b.prefix != "v")))
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

    /// A listing of `(tag, draft)`, marked prerelease as the alpha workflow
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

    /// GitHub's answer of Git references for `tags`, in its name order.
    fn tags(tags: &[&str]) -> Vec<u8> {
        let refs: Vec<String> = tags
            .iter()
            .map(|tag| format!(r#"{{"ref":"refs/tags/{tag}","object":{{"type":"commit"}}}}"#))
            .collect();
        format!("[{}]", refs.join(",")).into_bytes()
    }

    #[test]
    fn a_release_asks_for_tags_and_an_alpha_for_the_newest_releases() {
        assert_eq!(path(&version("1.0.0")), TAGS);
        assert_eq!(path(&version("1.0.0-alpha.12")), RELEASES);
    }

    #[test]
    fn a_release_runs_ahead_of_alphas_but_ignores_them() {
        let list = tags(&["enshwick-loader-v1.0.1", "enshwick-loader-v1.1.0-alpha.1"]);
        assert_eq!(found(&list, "1.0.0"), Some(version("1.0.1")));
        let list = tags(&["enshwick-loader-v1.1.0-alpha.1"]);
        assert_eq!(found(&list, "1.0.0"), None);
    }

    #[test]
    fn a_running_release_finds_a_stable_release_whatever_came_after_it() {
        let mut names: Vec<String> = (1..=40)
            .map(|n| format!("enshwick-loader-v1.2.0-alpha.{n}"))
            .collect();
        names.extend([
            "enshwick-loader-v1.0.0".into(),
            "enshwick-loader-v1.1.0".into(),
        ]);
        names.sort();
        let names: Vec<&str> = names.iter().map(String::as_str).collect();
        assert_eq!(found(&tags(&names), "1.0.0"), Some(version("1.1.0")));
    }

    #[test]
    fn a_running_release_finds_a_stable_release_under_either_prefix() {
        let list = tags(&["enshwick-v3.0.0", "v1.0.0-alpha.13", "v1.0.2"]);
        assert_eq!(found(&list, "1.0.0"), Some(version("1.0.2")));
        let list = tags(&["enshwick-loader-v1.0.1", "v1.0.2-alpha.1"]);
        assert_eq!(found(&list, "1.0.0"), Some(version("1.0.1")));
    }

    #[test]
    fn a_version_under_both_prefixes_links_the_release_plz_tag() {
        for names in [
            ["enshwick-loader-v1.1.0", "v1.1.0"],
            ["v1.1.0", "enshwick-loader-v1.1.0"],
        ] {
            let release = newer(&tags(&names), &version("1.0.0"), LOADER_TAGS)
                .unwrap()
                .unwrap();
            assert_eq!(release.page(), format!("{PAGE}enshwick-loader-v1.1.0"));
        }
    }

    #[test]
    fn each_side_reads_only_its_own_package_tags() {
        let list = tags(&["enshwick-loader-v2.0.0", "cargo-enshwick-v1.5.0"]);
        let tool = newer(&list, &version("1.0.0"), TOOL_TAGS).unwrap().unwrap();
        assert_eq!(tool.version, version("1.5.0"));
        assert_eq!(tool.page(), format!("{PAGE}cargo-enshwick-v1.5.0"));
        assert_eq!(found(&list, "1.0.0"), Some(version("2.0.0")));
    }

    #[test]
    fn a_reference_that_is_no_tag_is_skipped() {
        let list = br#"[{"ref":"refs/heads/enshwick-loader-v9.0.0"}]"#;
        assert_eq!(found(list, "1.0.0"), None);
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
    fn a_loader_release_tagged_by_release_plz_is_found_with_its_page() {
        let list = listing(&[
            ("enshwick-loader-v1.0.0", false),
            ("enshwick-v1.2.0", false),
        ]);
        let release = newer(&list, &version("1.0.0-alpha.12"), LOADER_TAGS)
            .unwrap()
            .unwrap();
        assert_eq!(release.version, version("1.0.0"));
        assert_eq!(
            release.page(),
            "https://github.com/enshwick/enshwick/releases/tag/enshwick-loader-v1.0.0"
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
}
