//! The links of the API crate's rendered docs, checked after rustdoc wrote
//! them: rustdoc checks an intra-doc link, but not a link written as a
//! relative URL, which a domain crate below `Load` must use to reach it. Each
//! `href` of a page under `target/doc/enshwick` that stays inside that
//! folder must reach a page that exists and, when it names one, an anchor
//! that page has. No network is used.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::FACADE;

/// Checks the facade's rendered docs under `root`'s `target/doc` and says
/// each broken link.
///
/// # Errors
///
/// When the docs are not built or a page does not read.
pub(crate) fn run(root: &Path) -> Result<bool, String> {
    let doc = root.join("target").join("doc");
    if !doc.join(FACADE).is_dir() {
        return Err(format!(
            "{} holds no docs; cargo doc writes them",
            doc.join(FACADE).display()
        ));
    }
    // Without `..` in it, a link's path resolves by plain pops.
    let doc = std::fs::canonicalize(&doc)
        .map_err(|error| format!("reading {}: {error}", doc.display()))?;
    let problems = check(&doc, FACADE)?;
    for problem in &problems {
        crate::say(&format!("doc link: {problem}"));
    }
    Ok(problems.is_empty())
}

/// The broken links of the pages under `doc`'s folder `krate`, one line each
/// naming the page, relative to `doc`, and the link.
///
/// # Errors
///
/// When a page does not read.
pub(crate) fn check(doc: &Path, krate: &str) -> Result<Vec<String>, String> {
    let mut pages = Vec::new();
    html_files(&doc.join(krate), &mut pages)?;
    pages.sort();
    let mut anchors: BTreeMap<PathBuf, Option<BTreeSet<String>>> = BTreeMap::new();
    let mut problems = Vec::new();
    for page in &pages {
        let text = read(page)?;
        let mut seen = BTreeSet::new();
        for href in attribute_values(&text, "href") {
            if !seen.insert(href.clone()) {
                continue;
            }
            let Some((target, fragment)) = resolve(doc, page, &href) else {
                continue;
            };
            // Links into another crate's pages are rustdoc's own, and those
            // pages exist only when the same run documented that crate. A
            // link straight into `doc` is a relative one gone a folder too
            // far up, as from a module's first paragraph on the crate index,
            // or one of rustdoc's shared pages, which exist.
            if !target.starts_with(doc.join(krate)) && target.parent() != Some(doc) {
                continue;
            }
            if !anchors.contains_key(&target) {
                let ids = if !target.is_file() {
                    None
                } else if target.extension().is_some_and(|e| e == "html") {
                    Some(
                        attribute_values(&read(&target)?, "id")
                            .into_iter()
                            .collect(),
                    )
                } else {
                    Some(BTreeSet::new())
                };
                anchors.insert(target.clone(), ids);
            }
            let ids = &anchors[&target];
            let shown = page.strip_prefix(doc).unwrap_or(page).display();
            match ids {
                None => problems.push(format!("{shown}: {href} reaches no page")),
                Some(ids) => {
                    if let Some(fragment) = fragment
                        && !ids.contains(&fragment)
                    {
                        problems.push(format!("{shown}: {href} names an anchor its page lacks"));
                    }
                }
            }
        }
    }
    Ok(problems)
}

/// The page and anchor `href` on `page` reaches, or `None` for a link this
/// check leaves alone: one with a scheme, a query, or a path out of `doc`.
fn resolve(doc: &Path, page: &Path, href: &str) -> Option<(PathBuf, Option<String>)> {
    let (path, fragment) = match href.split_once('#') {
        Some((path, fragment)) => (path, Some(fragment)),
        None => (href, None),
    };
    let scheme = path
        .find(':')
        .is_some_and(|colon| !path[..colon].contains('/'));
    if scheme || path.starts_with("//") || path.contains('?') {
        return None;
    }
    let mut target = if path.is_empty() {
        page.to_path_buf()
    } else {
        let mut target = page.parent()?.to_path_buf();
        for part in Path::new(path).components() {
            match part {
                Component::ParentDir => {
                    if !target.pop() {
                        return None;
                    }
                }
                Component::Normal(part) => target.push(part),
                Component::CurDir => {}
                Component::RootDir | Component::Prefix(_) => return None,
            }
        }
        target
    };
    if !target.starts_with(doc) {
        return None;
    }
    if path.ends_with('/') {
        target.push("index.html");
    }
    Some((
        target,
        fragment.filter(|f| !f.is_empty()).map(str::to_owned),
    ))
}

/// The values of every `name="..."` attribute in `html`, with the entities
/// rustdoc writes in attributes decoded.
fn attribute_values(html: &str, name: &str) -> Vec<String> {
    let opening = format!(" {name}=\"");
    html.match_indices(&opening)
        .filter_map(|(at, _)| {
            let start = at + opening.len();
            html[start..]
                .find('"')
                .map(|end| unescape(&html[start..start + end]))
        })
        .collect()
}

fn unescape(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
}

fn html_files(dir: &Path, into: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|error| format!("reading {}: {error}", dir.display()))?;
    for entry in entries {
        let path = entry
            .map_err(|error| format!("reading {}: {error}", dir.display()))?
            .path();
        if path.is_dir() {
            html_files(&path, into)?;
        } else if path.extension().is_some_and(|e| e == "html") {
            into.push(path);
        }
    }
    Ok(())
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|error| format!("reading {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn a_link_to_a_missing_page_or_anchor_is_named_and_good_links_pass() {
        let doc = std::env::temp_dir().join(format!("xtask-doc-links-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&doc);
        write(
            &doc.join("enshwick/struct.Load.html"),
            r##"<h4 id="method.add_item">add_item</h4><a href="#method.add_item">self</a>"##,
        );
        write(&doc.join("src/enshwick/lib.rs.html"), r#"<a id="1">1</a>"#);
        write(&doc.join("help.html"), "");
        // A module's first paragraph on the crate index, one folder up.
        write(
            &doc.join("enshwick/index.html"),
            concat!(
                r##"<a href="../help.html">rustdoc's own</a>"##,
                r##"<a href="../struct.Load.html#method.add_item">too far up</a>"##,
            ),
        );
        write(
            &doc.join("enshwick/content/index.html"),
            concat!(
                r##"<a href="../struct.Load.html#method.add_item">good</a>"##,
                r##"<a href="../../src/enshwick/lib.rs.html#1-4">lines</a>"##,
                r##"<a href="https://docs.rs/serde">away</a>"##,
                r##"<a href="//cdn.example/x.html#y">no scheme, another host</a>"##,
                r##"<a href="../struct.Load.html?search=x">a query</a>"##,
                r##"<a href="mailto:you@example.org">mail</a>"##,
                r##"<a href="../../enshwick_values/trait.Object.html#x">another crate</a>"##,
                r##"<a href="../../../outside.html">out of the docs</a>"##,
                r##"<a href="../struct.Load.html#method.add_itemx">bad anchor</a>"##,
                r##"<a href="../struct.World.html">bad page</a>"##,
            ),
        );
        let problems = check(&doc, "enshwick").unwrap();
        std::fs::remove_dir_all(&doc).unwrap();
        let page = Path::new("enshwick").join("content").join("index.html");
        let index = Path::new("enshwick").join("index.html");
        assert_eq!(
            problems,
            [
                format!(
                    "{}: ../struct.Load.html#method.add_itemx names an anchor its page lacks",
                    page.display()
                ),
                format!("{}: ../struct.World.html reaches no page", page.display()),
                format!(
                    "{}: ../struct.Load.html#method.add_item reaches no page",
                    index.display()
                ),
            ]
        );
    }

    #[test]
    fn entities_in_attributes_are_decoded() {
        assert_eq!(
            attribute_values(r#"<a href="a.html#x&amp;y" id="i&lt;T&gt;">"#, "href"),
            ["a.html#x&y"]
        );
        assert_eq!(
            attribute_values(r#"<a href="a.html" id="i&lt;T&gt;">"#, "id"),
            ["i<T>"]
        );
    }

    #[test]
    fn a_root_without_built_docs_is_an_error_not_a_pass() {
        let root = std::env::temp_dir().join(format!("xtask-no-docs-{}", std::process::id()));
        let _gone = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let outcome = run(&root);
        std::fs::remove_dir_all(&root).unwrap();
        assert!(outcome.unwrap_err().contains("holds no docs"));
    }
}
