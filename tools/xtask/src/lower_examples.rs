//! The doc examples of the internal crates under `crates/internal/`, which
//! compile in their own crate but read as the facade's. An example that names
//! `enshwick::` names its own crate so by a hidden alias line,
//! `# extern crate <crate> as enshwick;`. One that names items above its
//! crate sits in a hidden wrapper that keeps it out of its crate's doctests,
//! and the facade's `lower_examples.rs` holds an equal copy, which compiles
//! it. A crate that depends on the facade names the real `enshwick` and is
//! left out. `cargo xtask alias-examples` writes the missing alias lines.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The folder of the internal crates, relative to the workspace root.
const INTERNAL: &str = "crates/internal";
/// The facade's copies of the wrapped examples, relative to the workspace root.
const COPIES: &str = "crates/enshwick/src/lower_examples.rs";
/// The facade's package name.
const FACADE: &str = "enshwick";
/// The first line of a wrapped example, as its doc comment holds it.
const WRAPPER_OPEN: &str = "# #[cfg(any())] fn in_enshwick() {";
/// The last line of a wrapped example.
const WRAPPER_CLOSE: &str = "# }";
/// What an example names when it needs the alias line.
const NAMES_FACADE: &str = "enshwick::";
/// The fence tokens of a code block rustdoc compiles as Rust, besides the
/// `edition` ones.
const RUST_TOKENS: [&str; 6] = [
    "rust",
    "should_panic",
    "no_run",
    "compile_fail",
    "standalone_crate",
    "test_harness",
];

/// A code block of a doc comment.
#[derive(Debug, PartialEq, Eq)]
struct Example {
    /// The line of its opening fence, from 0.
    fence: usize,
    /// What follows the opening fence's backticks, trimmed.
    info: String,
    /// The text of each line inside the fences, after the comment marker and
    /// its one space.
    lines: Vec<String>,
    /// The comment marker and the indent before it, as the fence line has
    /// them, such as `    /// `.
    prefix: String,
}

impl Example {
    /// Whether rustdoc compiles it as Rust.
    fn compiles(&self) -> bool {
        self.info
            .split([',', ' ', '\t'])
            .filter(|token| !token.is_empty())
            .all(|token| RUST_TOKENS.contains(&token) || token.starts_with("edition"))
    }

    /// Whether it opens with the wrapper.
    fn wrapped(&self) -> bool {
        self.lines
            .first()
            .is_some_and(|line| line.trim_start().starts_with(WRAPPER_OPEN))
    }

    /// The crate its alias line names, if it has one.
    fn alias(&self) -> Option<&str> {
        self.lines.iter().find_map(|line| {
            line.trim()
                .strip_prefix("# extern crate ")?
                .strip_suffix(" as enshwick;")
        })
    }

    /// The fence and the text inside the wrapper, which its copy repeats;
    /// `None` without the closing line.
    fn wrapped_text(&self) -> Option<(String, String)> {
        let (last, inner) = self.lines.get(1..)?.split_last()?;
        (last.trim() == WRAPPER_CLOSE).then(|| (self.info.clone(), inner.join("\n")))
    }
}

/// The code blocks of the doc comments (`///`, `//!`) of the Rust `text`.
fn examples(text: &str) -> Vec<Example> {
    let mut found = Vec::new();
    let mut open: Option<Example> = None;
    for (at, line) in text.lines().enumerate() {
        let Some((prefix, doc)) = doc_line(line) else {
            open = None;
            continue;
        };
        let fence = doc.trim_start().starts_with("```");
        match open.take() {
            None if fence => {
                open = Some(Example {
                    fence: at,
                    info: doc.trim_start().trim_start_matches('`').trim().to_owned(),
                    lines: Vec::new(),
                    prefix: prefix.to_owned(),
                });
            }
            None => {}
            Some(example) if fence => found.push(example),
            Some(mut example) => {
                example.lines.push(doc.to_owned());
                open = Some(example);
            }
        }
    }
    found
}

/// The marker with its indent and one space, and the text after them, of a
/// doc comment `line`.
fn doc_line(line: &str) -> Option<(&str, &str)> {
    let indent = line.len() - line.trim_start().len();
    let rest = &line[indent..];
    let marked = ["///", "//!"]
        .iter()
        .find(|marker| rest.starts_with(*marker) && !rest.starts_with("////"))?;
    let text = &rest[marked.len()..];
    let text_at = indent + marked.len() + usize::from(text.starts_with(' '));
    Some((&line[..text_at], &line[text_at..]))
}

/// An internal crate whose examples the rules hold.
#[derive(Debug)]
struct Crate {
    /// Its library name, which the alias line names.
    lib: String,
    /// Its Rust sources under `src/`.
    sources: Vec<PathBuf>,
}

/// The internal crates under `root` that do not depend on the facade.
fn crates(root: &Path) -> Result<Vec<Crate>, String> {
    let folder = root.join(INTERNAL);
    let mut found = Vec::new();
    let entries = std::fs::read_dir(&folder).map_err(|e| format!("{INTERNAL}: {e}"))?;
    for entry in entries {
        let dir = entry.map_err(|e| format!("{INTERNAL}: {e}"))?.path();
        let manifest = dir.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let text = read(&manifest)?;
        let table: toml::Table = toml::from_str(&text)
            .map_err(|e| format!("{}: {}", manifest.display(), e.message()))?;
        let Some(name) = table
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
        else {
            continue;
        };
        let above = table
            .get("dependencies")
            .and_then(toml::Value::as_table)
            .is_some_and(|dependencies| dependencies.contains_key(FACADE));
        if above {
            continue;
        }
        let mut sources = Vec::new();
        rust_files(&dir.join("src"), &mut sources)?;
        sources.sort();
        found.push(Crate {
            lib: name.replace('-', "_"),
            sources,
        });
    }
    found.sort_by(|a, b| a.lib.cmp(&b.lib));
    Ok(found)
}

/// Adds the Rust files under `dir` to `out`.
fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    for entry in entries {
        let path = entry.map_err(|e| format!("{}: {e}", dir.display()))?.path();
        if path.is_dir() {
            rust_files(&path, out)?;
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
    Ok(())
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// `path` relative to `root`, with `/`.
fn shown(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Where a wrapped example or a copy is, by its fence and body.
type Places = BTreeMap<(String, String), Vec<String>>;

/// The problems of the internal crates' examples under `root`, one line
/// each.
pub(crate) fn check(root: &Path) -> Vec<String> {
    let crates = match crates(root) {
        Ok(crates) => crates,
        Err(error) => return vec![error],
    };
    let mut problems = Vec::new();
    let mut wrapped = Places::new();
    for krate in &crates {
        for source in &krate.sources {
            let text = match read(source) {
                Ok(text) => text,
                Err(error) => {
                    problems.push(error);
                    continue;
                }
            };
            let file = shown(source, root);
            for example in examples(&text).iter().filter(|e| e.compiles()) {
                let at = format!("{file}:{}", example.fence + 1);
                problems.extend(example_problem(example, &krate.lib, &at, &mut wrapped));
            }
        }
    }
    let copies_path = root.join(COPIES);
    let copies = match read(&copies_path) {
        Ok(text) => examples(&text),
        Err(error) => {
            problems.push(error);
            return problems;
        }
    };
    let mut copy_places = Places::new();
    for copy in copies {
        let key = (copy.info, copy.lines.join("\n"));
        copy_places
            .entry(key)
            .or_default()
            .push(format!("{COPIES}:{}", copy.fence + 1));
    }
    problems.extend(unpaired(&wrapped, &copy_places));
    problems
}

/// The problem of one compiled `example` of the crate `lib` at `at`, if any;
/// a wrapped one joins `wrapped`.
fn example_problem(example: &Example, lib: &str, at: &str, wrapped: &mut Places) -> Option<String> {
    if example.wrapped() {
        let Some(key) = example.wrapped_text() else {
            return Some(format!(
                "{at}: the wrapper does not end with `{WRAPPER_CLOSE}`"
            ));
        };
        wrapped.entry(key).or_default().push(at.to_owned());
        return None;
    }
    match example.alias() {
        Some(named) if named != lib => Some(format!(
            "{at}: the alias line names {named}; this example is {lib}'s"
        )),
        None if example.lines.iter().any(|line| line.contains(NAMES_FACADE)) => Some(format!(
            "{at}: no `# extern crate {lib} as enshwick;` line; `cargo xtask alias-examples` writes it"
        )),
        _ => None,
    }
}

/// The wrapped examples without an equal copy and the copies without an
/// equal wrapped example, paired one to one.
fn unpaired(wrapped: &Places, copy_places: &Places) -> Vec<String> {
    let mut problems = Vec::new();
    for (key, places) in wrapped {
        let copy_count = copy_places.get(key).map_or(0, Vec::len);
        for at in places.iter().skip(copy_count) {
            problems.push(format!(
                "{at}: this example names items above its crate, and {COPIES} holds no equal copy"
            ));
        }
    }
    for (key, places) in copy_places {
        let originals = wrapped.get(key).map_or(0, Vec::len);
        for at in places.iter().skip(originals) {
            problems.push(format!(
                "{at}: no wrapped example of {INTERNAL} equals this copy; bring it in line or remove it"
            ));
        }
    }
    problems
}

/// Writes the missing alias lines into the internal crates under `root`.
///
/// # Errors
///
/// When a crate's manifest or a source does not read or write.
pub(crate) fn run(root: &Path) -> Result<bool, String> {
    let mut written = 0usize;
    for krate in crates(root)? {
        for source in &krate.sources {
            let text = read(source)?;
            let (aliased, count) = with_aliases(&text, &krate.lib);
            if count > 0 {
                std::fs::write(source, aliased)
                    .map_err(|e| format!("{}: {e}", source.display()))?;
                written += count;
            }
        }
    }
    crate::say(&format!("{written} alias lines written"));
    Ok(true)
}

/// The Rust `text` with an alias line naming `lib` after the opening fence
/// of each compiled example that names `enshwick::` without one or the
/// wrapper, and how many it added.
fn with_aliases(text: &str, lib: &str) -> (String, usize) {
    let fences: Vec<(usize, String)> = examples(text)
        .into_iter()
        .filter(|e| {
            e.compiles()
                && !e.wrapped()
                && e.alias().is_none()
                && e.lines.iter().any(|line| line.contains(NAMES_FACADE))
        })
        .map(|e| (e.fence, e.prefix))
        .collect();
    let mut out = String::with_capacity(text.len() + fences.len() * 48);
    let mut next = fences.iter().peekable();
    for (at, line) in text.split_inclusive('\n').enumerate() {
        out.push_str(line);
        if let Some((_, prefix)) = next.next_if(|(fence, _)| *fence == at) {
            out.push_str(prefix);
            out.push_str("# extern crate ");
            out.push_str(lib);
            out.push_str(" as enshwick;\n");
        }
    }
    (out, fences.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "\
//! A crate.
//!
//! ```
//! use enshwick::Load;
//! ```

/// A type.
///
/// ```rust,standalone_crate
/// # #[cfg(any())] fn in_enshwick() { // names items above this crate: enshwick's doctests compile it
/// let load = enshwick::Load::new();
/// # }
/// ```
///
/// ```toml
/// [package.metadata.enshwick]
/// ```
pub struct Thing;

impl Thing {
    /// Makes one.
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// let thing = enshwick::Thing::new();
    /// ```
    ///
    /// ```
    /// let two = 1 + 1;
    /// ```
    pub fn new() -> Self {
        Self
    }
}
";

    #[test]
    fn examples_are_read_with_their_fence_and_marker() {
        let found = examples(SOURCE);
        assert_eq!(found.len(), 5);
        assert_eq!(found[0].prefix, "//! ");
        assert_eq!(found[0].lines, ["use enshwick::Load;"]);
        assert_eq!(found[1].info, "rust,standalone_crate");
        assert!(found[1].wrapped());
        assert!(!found[2].compiles());
        assert_eq!(found[3].prefix, "    /// ");
        assert_eq!(found[3].alias(), Some("enshwick_core"));
        assert!(found[4].compiles());
    }

    #[test]
    fn a_wrapped_example_compares_by_its_fence_and_the_text_inside_the_wrapper() {
        let found = examples(SOURCE);
        assert_eq!(
            found[1].wrapped_text(),
            Some((
                "rust,standalone_crate".to_owned(),
                "let load = enshwick::Load::new();".to_owned()
            ))
        );
        let open =
            examples("/// ```\n/// # #[cfg(any())] fn in_enshwick() {\n/// let a = 1;\n/// ```\n");
        assert_eq!(open[0].wrapped_text(), None);
    }

    #[test]
    fn an_example_naming_the_facade_needs_an_alias_naming_its_own_crate() {
        let found = examples(SOURCE);
        let mut wrapped = Places::new();
        assert_eq!(
            example_problem(&found[0], "enshwick_core", "a.rs:3", &mut wrapped),
            Some("a.rs:3: no `# extern crate enshwick_core as enshwick;` line; `cargo xtask alias-examples` writes it".to_owned())
        );
        assert_eq!(
            example_problem(&found[3], "enshwick_abi", "a.rs:24", &mut wrapped),
            Some(
                "a.rs:24: the alias line names enshwick_core; this example is enshwick_abi's"
                    .to_owned()
            )
        );
        assert_eq!(
            example_problem(&found[3], "enshwick_core", "a.rs:24", &mut wrapped),
            None
        );
        assert_eq!(
            example_problem(&found[4], "enshwick_core", "a.rs:29", &mut wrapped),
            None
        );
        assert_eq!(
            example_problem(&found[1], "enshwick_core", "a.rs:9", &mut wrapped),
            None
        );
        assert_eq!(wrapped.values().flatten().collect::<Vec<_>>(), ["a.rs:9"]);
    }

    #[test]
    fn wrapped_examples_and_copies_pair_one_to_one() {
        let key = |body: &str| (String::new(), body.to_owned());
        let wrapped = Places::from([
            (key("a"), vec!["x.rs:1".to_owned(), "x.rs:9".to_owned()]),
            (key("b"), vec!["y.rs:4".to_owned()]),
        ]);
        let copied = Places::from([
            (key("a"), vec!["c.rs:2".to_owned()]),
            (key("c"), vec!["c.rs:7".to_owned()]),
        ]);
        assert_eq!(
            unpaired(&wrapped, &copied),
            [
                format!(
                    "x.rs:9: this example names items above its crate, and {COPIES} holds no equal copy"
                ),
                format!(
                    "y.rs:4: this example names items above its crate, and {COPIES} holds no equal copy"
                ),
                format!(
                    "c.rs:7: no wrapped example of {INTERNAL} equals this copy; bring it in line or remove it"
                ),
            ]
        );
    }

    #[test]
    fn the_writer_adds_an_alias_only_where_one_is_missing() {
        let (written, count) = with_aliases(SOURCE, "enshwick_core");
        assert_eq!(count, 1);
        assert!(written.starts_with("//! A crate.\n//!\n//! ```\n//! # extern crate enshwick_core as enshwick;\n//! use enshwick::Load;\n"));
        assert_eq!(with_aliases(&written, "enshwick_core").1, 0);
        assert_eq!(
            written.len(),
            SOURCE.len() + "//! # extern crate enshwick_core as enshwick;\n".len()
        );
    }
}
