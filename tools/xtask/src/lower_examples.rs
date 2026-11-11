//! The doc examples of the internal crates under `crates/internal/`, which
//! compile in their own crate but read as the facade's. An example that names
//! `enshwick::` names its own crate so by a hidden alias line,
//! `# extern crate <crate> as enshwick;`. One that names items above its
//! crate sits in a hidden wrapper that keeps it out of its crate's doctests,
//! and the facade's `lower_examples.rs` holds an equal copy, which compiles
//! it. A crate that depends on the facade names the real `enshwick` and is
//! left out. `cargo xtask alias-examples` writes the missing alias lines;
//! `cargo xtask wrap-examples <doctest log> [--cfg <feature>]` wraps the
//! examples a crate's failed doctests name and writes their copies, one
//! file per crate and feature.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::affected::Package;

/// The folder of the internal crates, relative to the workspace root.
const INTERNAL: &str = "crates/internal";
/// The facade's module of the copies of the wrapped examples, relative to
/// the workspace root, without `.rs`: its file and the files of its folder,
/// one per internal crate and feature its examples need, such as `base.rs`
/// and `base_unstable.rs`.
const COPIES: &str = "crates/enshwick/src/lower_examples";
/// The facade's package name.
const FACADE: &str = "enshwick";
/// The first line of a wrapped example, as its doc comment holds it.
const WRAPPER_OPEN: &str = "# #[cfg(any())] fn in_enshwick() {";
/// The last line of a wrapped example.
const WRAPPER_CLOSE: &str = "# }";
/// What an example names when it needs the alias line.
const NAMES_FACADE: &str = "enshwick::";
/// The fence tokens rustdoc reads as Rust attributes, besides `rust`,
/// `ignore`, the `ignore-<target>`, `edition` and error code ones.
const RUST_TOKENS: [&str; 5] = [
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
    /// What precedes the opening fence's backticks: the indent, the comment
    /// marker and the spaces after it, such as `    /// ` or, in a list
    /// item, `///   `.
    prefix: String,
}

impl Example {
    /// Whether rustdoc compiles it as Rust. By rustdoc's rule a fence is Rust
    /// without a token of another language, or with `rust` or a Rust
    /// attribute before the first such token; `ignore` compiles nothing.
    fn compiles(&self) -> bool {
        let (mut other, mut rust, mut ignored) = (false, false, false);
        for token in self.info.split([',', ' ', '\t']).filter(|t| !t.is_empty()) {
            match token {
                "rust" => rust = true,
                "ignore" => ignored = true,
                _ if token.starts_with("edition") => {}
                _ if RUST_TOKENS.contains(&token)
                    || token.starts_with("ignore-")
                    || is_error_code(token) =>
                {
                    rust |= !other;
                }
                _ => other = true,
            }
        }
        !ignored && (rust || !other)
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

/// The code blocks of the doc comments (`///`, `//!`) of the Rust `text`; a
/// block left open runs to the end of its comment, as rustdoc reads it.
fn examples(text: &str) -> Vec<Example> {
    let mut found = Vec::new();
    let mut open: Option<Example> = None;
    for (at, line) in text.lines().enumerate() {
        let Some(doc) = doc_line(line) else {
            found.extend(open.take());
            continue;
        };
        let code = doc.trim_start();
        let fence = code.starts_with("```");
        match open.take() {
            None if fence => {
                open = Some(Example {
                    fence: at,
                    info: code.trim_start_matches('`').trim().to_owned(),
                    lines: Vec::new(),
                    prefix: line[..line.len() - code.len()].to_owned(),
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
    found.extend(open);
    found
}

/// The text of a doc comment `line` after its marker and one space.
fn doc_line(line: &str) -> Option<&str> {
    let rest = line.trim_start();
    let marked = ["///", "//!"]
        .iter()
        .find(|marker| rest.starts_with(*marker) && !rest.starts_with("////"))?;
    let text = &rest[marked.len()..];
    Some(text.strip_prefix(' ').unwrap_or(text))
}

/// Whether `token` is an error code a `compile_fail` example expects, such
/// as `E0428`.
fn is_error_code(token: &str) -> bool {
    token.len() == 5 && token.starts_with('E') && token.bytes().skip(1).all(|b| b.is_ascii_digit())
}

/// An internal crate whose examples the rules hold.
#[derive(Debug)]
struct Crate {
    /// Its library name, which the alias line names.
    lib: String,
    /// Its Rust sources under `src/`.
    sources: Vec<PathBuf>,
}

/// The internal crates among `packages` under `root` that do not depend on
/// the facade in any dependency kind, each by its library's name.
fn crates(root: &Path, packages: &[Package]) -> Result<Vec<Crate>, String> {
    let mut found = Vec::new();
    for package in packages {
        let internal = package
            .dir
            .strip_prefix(INTERNAL)
            .is_some_and(|rest| rest.starts_with('/'));
        let above = package.deps.iter().any(|dep| dep == FACADE);
        let Some(lib) = package.lib.as_ref().filter(|_| internal && !above) else {
            continue;
        };
        let mut sources = Vec::new();
        rust_files(&root.join(&package.dir).join("src"), &mut sources)?;
        sources.sort();
        found.push(Crate {
            lib: lib.clone(),
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
pub(crate) fn check(root: &Path, packages: &[Package]) -> Vec<String> {
    let crates = match crates(root, packages) {
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
    let mut copy_files = vec![root.join(format!("{COPIES}.rs"))];
    let folder = root.join(COPIES);
    if folder.is_dir()
        && let Err(error) = rust_files(&folder, &mut copy_files)
    {
        problems.push(error);
        return problems;
    }
    copy_files[1..].sort();
    let mut copy_places = Places::new();
    let mut module = String::new();
    for (at, path) in copy_files.iter().enumerate() {
        let text = match read(path) {
            Ok(text) => text,
            Err(error) => {
                problems.push(error);
                continue;
            }
        };
        let file = shown(path, root);
        if at == 0 {
            module.clone_from(&text);
        } else if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
            && !module
                .lines()
                .any(|line| line.trim() == format!("mod {stem};"))
        {
            // An undeclared file's copies compile nowhere, though they pair.
            problems.push(format!(
                "{file}: {COPIES}.rs does not declare it, so its copies compile nowhere; add `mod {stem};`"
            ));
        }
        for copy in examples(&text) {
            let key = (copy.info, copy.lines.join("\n"));
            copy_places
                .entry(key)
                .or_default()
                .push(format!("{file}:{}", copy.fence + 1));
        }
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
/// When a source does not read or write.
pub(crate) fn run(root: &Path, packages: &[Package]) -> Result<bool, String> {
    let mut written = 0usize;
    for krate in crates(root, packages)? {
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

/// A failed doctest as rustdoc lists it: `<file> - <item> (line <n>)`, the
/// line its opening fence's, from 1.
#[derive(Debug, PartialEq, Eq)]
struct Failed {
    file: String,
    item: String,
    fence: usize,
}

/// The failed doctests of a `cargo test --doc` log, each once.
fn failed(log: &str) -> Vec<Failed> {
    let mut found: Vec<Failed> = log
        .lines()
        .filter_map(|line| {
            let (file, rest) = line.trim().split_once(" - ")?;
            let (item, at) = rest.strip_suffix(')')?.rsplit_once(" (line ")?;
            Some(Failed {
                file: file.replace('\\', "/"),
                item: item.to_owned(),
                fence: at.parse().ok()?,
            })
        })
        .collect();
    found.sort_by(|a, b| (&a.file, a.fence).cmp(&(&b.file, b.fence)));
    found.dedup_by(|a, b| a.file == b.file && a.fence == b.fence);
    found
}

/// `text` with the example whose opening fence is on line `fence` (from 0)
/// in the wrapper, its alias line gone, and that example's copy, labelled
/// `item`; `None` when no unwrapped example opens there.
fn wrapped(text: &str, fence: usize, item: &str) -> Option<(String, String)> {
    let example = examples(text)
        .into_iter()
        .find(|e| e.fence == fence && !e.wrapped())?;
    let close = fence + example.lines.len() + 1;
    let lines: Vec<&String> = example
        .lines
        .iter()
        .filter(|line| !line.trim().starts_with("# extern crate "))
        .collect();
    let mut copy = format!("//!\n//! `{item}`:\n//!\n//! ```{}\n", example.info);
    for line in &lines {
        copy.push_str(if line.is_empty() { "//!" } else { "//! " });
        copy.push_str(line);
        copy.push('\n');
    }
    copy.push_str("//! ```\n");
    let prefix = &example.prefix;
    let mut out = String::with_capacity(text.len() + 160);
    for (at, line) in text.split_inclusive('\n').enumerate() {
        if at > fence && at < close {
            continue;
        }
        if at == close {
            let _written = writeln!(
                out,
                "{prefix}{WRAPPER_OPEN} // names items above this crate: enshwick's doctests compile it"
            );
            for kept in &lines {
                out.push_str(prefix.trim_end());
                if !kept.is_empty() {
                    out.push(' ');
                    out.push_str(kept);
                }
                out.push('\n');
            }
            let _written = writeln!(out, "{prefix}{WRAPPER_CLOSE}");
        }
        out.push_str(line);
    }
    Some((out, copy))
}

/// Wraps the examples a doctest log of an internal crate names as failed,
/// and writes their copies into the facade's file for that crate and
/// `feature`.
///
/// # Errors
///
/// When the log or a source does not read or write, or an example it names
/// is not found unwrapped.
pub(crate) fn wrap(root: &Path, packages: &[Package], args: &[String]) -> Result<bool, String> {
    let (log, feature) = match args {
        [log] => (log, None),
        [log, flag, feature] if flag == "--cfg" => (log, Some(feature.as_str())),
        _ => return Err("usage: cargo xtask wrap-examples <doctest log> [--cfg <feature>]".into()),
    };
    let crates = crates(root, packages)?;
    let mut by_file: BTreeMap<String, Vec<Failed>> = BTreeMap::new();
    for one in failed(&read(Path::new(log))?) {
        by_file.entry(one.file.clone()).or_default().push(one);
    }
    // Each copies file by name, with its crate's short name and the copies to add.
    let mut copies: BTreeMap<String, (String, String)> = BTreeMap::new();
    // The wrapped sources, written only once every copy is.
    let mut sources = Vec::new();
    for (file, mut list) in by_file {
        let path = root.join(&file);
        let lib = crates
            .iter()
            .find(|krate| krate.sources.contains(&path))
            .map(|krate| krate.lib.clone())
            .ok_or_else(|| format!("{file}: no internal crate below the facade holds it"))?;
        let mut text = read(&path)?;
        let mut file_copies = Vec::new();
        list.sort_by_key(|one| core::cmp::Reverse(one.fence));
        for one in &list {
            let (next, copy) = one
                .fence
                .checked_sub(1)
                .and_then(|fence| wrapped(&text, fence, &one.item))
                .ok_or_else(|| format!("{file}:{}: no unwrapped example opens here", one.fence))?;
            text = next;
            file_copies.push(copy);
        }
        sources.push((path, text));
        let short = lib.strip_prefix("enshwick_").unwrap_or(&lib);
        let name = feature.map_or_else(|| short.to_owned(), |f| format!("{short}_{f}"));
        let out = copies
            .entry(name)
            .or_insert_with(|| (short.to_owned(), String::new()));
        for copy in file_copies.into_iter().rev() {
            out.1.push_str(&copy);
        }
    }
    for (name, (short, added)) in copies {
        let path = root.join(COPIES).join(format!("{name}.rs"));
        let mut text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                crate::say(&format!(
                    "{}: new; declare it in the facade's lower_examples.rs",
                    path.display()
                ));
                format!(
                    "//! The examples of `enshwick-{short}` that name items above its crate,\n//! compiled here as written.\n"
                )
            }
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        text.push_str(&added);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    for (path, text) in sources {
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(true)
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

    #[test]
    fn a_fence_left_open_runs_to_the_end_of_its_comment() {
        let found =
            examples("/// ```\n/// use enshwick::Load;\nfn f() {}\n//! ```\n//! let a = 1;");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].lines, ["use enshwick::Load;"]);
        assert_eq!(found[1].lines, ["let a = 1;"]);
    }

    #[test]
    fn a_failed_doctest_is_read_once_from_its_log_line() {
        let log = "failures:\n    crates\\internal\\base\\src\\ecs.rs - ecs::Ecs (line 27)\n    crates\\internal\\base\\src\\ecs.rs - ecs::Ecs (line 27)\n    crates/internal/base/src/a.rs - a::B<T>::c (line 3)\n    crates/internal/base/src/a.rs - a::D (line 9)\ntest result: FAILED.";
        assert_eq!(
            failed(log),
            [
                Failed {
                    file: "crates/internal/base/src/a.rs".to_owned(),
                    item: "a::B<T>::c".to_owned(),
                    fence: 3,
                },
                Failed {
                    file: "crates/internal/base/src/a.rs".to_owned(),
                    item: "a::D".to_owned(),
                    fence: 9,
                },
                Failed {
                    file: "crates/internal/base/src/ecs.rs".to_owned(),
                    item: "ecs::Ecs".to_owned(),
                    fence: 27,
                },
            ]
        );
    }

    #[test]
    fn a_wrapped_example_drops_its_alias_and_pairs_with_its_copy() -> Result<(), String> {
        let text = "/// A thing.\n///\n/// ```\n/// # extern crate enshwick_base as enshwick;\n/// let load = enshwick::Load::new();\n///\n/// let _ = load;\n/// ```\npub struct Thing;\n";
        let (out, copy) = wrapped(text, 2, "Thing").ok_or("the example opens on line 3")?;
        assert_eq!(
            out,
            "/// A thing.\n///\n/// ```\n/// # #[cfg(any())] fn in_enshwick() { // names items above this crate: enshwick's doctests compile it\n/// let load = enshwick::Load::new();\n///\n/// let _ = load;\n/// # }\n/// ```\npub struct Thing;\n"
        );
        assert_eq!(
            copy,
            "//!\n//! `Thing`:\n//!\n//! ```\n//! let load = enshwick::Load::new();\n//!\n//! let _ = load;\n//! ```\n"
        );
        let mut wrapped_places = Places::new();
        let original = &examples(&out)[0];
        assert_eq!(
            example_problem(original, "enshwick_base", "a.rs:3", &mut wrapped_places),
            None
        );
        let mut copy_places = Places::new();
        for copy in examples(&copy) {
            copy_places
                .entry((copy.info, copy.lines.join("\n")))
                .or_default()
                .push("c.rs".to_owned());
        }
        assert!(unpaired(&wrapped_places, &copy_places).is_empty());
        assert!(wrapped(&out, 2, "Thing").is_none(), "wrapped once");
        Ok(())
    }

    #[test]
    fn the_writer_indents_the_alias_as_its_fence_inside_a_list() {
        let text =
            "/// - An item:\n///\n///   ```\n///   let load = enshwick::Load::new();\n///   ```\n";
        let (written, count) = with_aliases(text, "enshwick_core");
        assert_eq!(count, 1);
        assert!(written.contains(
            "///   ```\n///   # extern crate enshwick_core as enshwick;\n///   let load"
        ));
    }

    #[test]
    fn a_fence_of_rust_attributes_or_an_edition_compiles_and_another_language_does_not() {
        let fence = |info: &str| Example {
            fence: 0,
            info: info.to_owned(),
            lines: Vec::new(),
            prefix: String::new(),
        };
        assert!(fence("").compiles());
        assert!(fence("edition2024").compiles());
        assert!(fence("rust,no_run").compiles());
        assert!(fence("compile_fail,E0428").compiles());
        assert!(fence("ignore-windows").compiles());
        assert!(fence("no_run,text").compiles());
        assert!(fence("text,rust").compiles());
        assert!(!fence("toml").compiles());
        assert!(!fence("text,no_run").compiles());
        assert!(!fence("text,E0428").compiles());
        assert!(!fence("ignore").compiles());
    }

    #[test]
    fn a_tree_is_checked_below_the_facade_and_the_writer_leaves_it_passing() {
        let root = std::env::temp_dir().join(format!("xtask-lower-{}", std::process::id()));
        let write = |path: &str, text: &str| {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        let package = |dir: &str, lib: Option<&str>, deps: &[&str]| Package {
            name: dir.rsplit('/').next().unwrap_or_default().to_owned(),
            version: String::new(),
            dir: dir.to_owned(),
            deps: deps.iter().map(|dep| (*dep).to_owned()).collect(),
            lib: lib.map(str::to_owned),
            windows_only: false,
            windows_tests: false,
            unstable: false,
        };
        let example = "/// ```\n/// let load = enshwick::Load::new();\n/// ```\npub struct Item;\n";
        write("crates/internal/core/src/deep/item.rs", example);
        write("crates/internal/values/src/lib.rs", example);
        write("crates/internal/fake/src/lib.rs", example);
        write("crates/runtime/stub/src/lib.rs", example);
        write("crates/internal/notes/README.md", "No crate.\n");
        write("crates/internal-tools/src/lib.rs", example);
        write(&format!("{COPIES}.rs"), "//! The copies.\n\nmod gone;\n");
        write(
            &format!("{COPIES}/gone.rs"),
            "//! ```\n//! let gone = 1;\n//! ```\n",
        );
        write(&format!("{COPIES}/stray.rs"), "//! Declared nowhere.\n");
        let packages = [
            package("crates/internal/core", Some("enshwick_core"), &[]),
            package("crates/internal/values", Some("enshwick_values"), &[]),
            package("crates/internal/fake", Some("enshwick_fake"), &[FACADE]),
            package("crates/runtime/stub", Some("powrprof"), &[]),
            package("crates/internal/notes", None, &[]),
            package("crates/internal-tools", Some("tools"), &[]),
        ];
        let before = check(&root, &packages);
        let written = run(&root, &packages);
        let after = check(&root, &packages);
        let item =
            std::fs::read_to_string(root.join("crates/internal/core/src/deep/item.rs")).unwrap();
        let stub = std::fs::read_to_string(root.join("crates/internal/values/src/lib.rs")).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert_eq!(
            before,
            [
                "crates/internal/core/src/deep/item.rs:1: no `# extern crate enshwick_core as enshwick;` line; `cargo xtask alias-examples` writes it".to_owned(),
                "crates/internal/values/src/lib.rs:1: no `# extern crate enshwick_values as enshwick;` line; `cargo xtask alias-examples` writes it".to_owned(),
                format!("{COPIES}/stray.rs: {COPIES}.rs does not declare it, so its copies compile nowhere; add `mod stray;`"),
                format!("{COPIES}/gone.rs:1: no wrapped example of {INTERNAL} equals this copy; bring it in line or remove it"),
            ]
        );
        assert_eq!(written, Ok(true));
        assert!(
            item.contains("/// ```\n/// # extern crate enshwick_core as enshwick;\n/// let load")
        );
        assert!(stub.contains("/// # extern crate enshwick_values as enshwick;"));
        assert_eq!(
            after,
            [
                format!(
                    "{COPIES}/stray.rs: {COPIES}.rs does not declare it, so its copies compile nowhere; add `mod stray;`"
                ),
                format!(
                    "{COPIES}/gone.rs:1: no wrapped example of {INTERNAL} equals this copy; bring it in line or remove it"
                ),
            ]
        );
    }

    #[test]
    fn wrap_examples_wraps_a_failed_example_and_adds_its_copy_to_its_crate_s_file() {
        let root = std::env::temp_dir().join(format!("xtask-wrap-{}", std::process::id()));
        let source = "crates/internal/base/src/lib.rs";
        let path = root.join(source);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let example = |name: &str| {
            format!(
                "/// ```\n/// # extern crate enshwick_base as enshwick;\n/// let {name} = enshwick::Load::new();\n/// ```\npub struct {name};\n"
            )
        };
        std::fs::write(&path, format!("{}{}", example("a"), example("b"))).unwrap();
        let log = root.join("doctests.log");
        std::fs::write(&log, format!("failures:\n    {source} - a (line 1)\n")).unwrap();
        let packages = [Package {
            name: "base".to_owned(),
            version: String::new(),
            dir: "crates/internal/base".to_owned(),
            deps: Vec::new(),
            lib: Some("enshwick_base".to_owned()),
            windows_only: false,
            windows_tests: false,
            unstable: false,
        }];
        let log = log.display().to_string();
        let usage = wrap(
            &root,
            &packages,
            &[log.clone(), "--feature".to_owned(), "x".to_owned()],
        );
        let first = wrap(
            &root,
            &packages,
            &[log.clone(), "--cfg".to_owned(), "unstable".to_owned()],
        );
        std::fs::write(root.join(&log), format!("    {source} - b (line 7)\n")).unwrap();
        let second = wrap(
            &root,
            &packages,
            &[log.clone(), "--cfg".to_owned(), "unstable".to_owned()],
        );
        let plain = wrap(&root, &packages, std::slice::from_ref(&log));
        std::fs::write(root.join(COPIES).join("base_odd.rs"), [0xff, 0xfe]).unwrap();
        let so_far = std::fs::read_to_string(&path).unwrap();
        std::fs::write(&path, format!("{so_far}{}", example("c"))).unwrap();
        let c_at = so_far.lines().count() + 1;
        std::fs::write(root.join(&log), format!("    {source} - c (line {c_at})\n")).unwrap();
        let unreadable = wrap(
            &root,
            &packages,
            &[log.clone(), "--cfg".to_owned(), "odd".to_owned()],
        );
        let copies = std::fs::read_to_string(root.join(COPIES).join("base_unstable.rs")).unwrap();
        let odd = std::fs::read(root.join(COPIES).join("base_odd.rs")).unwrap();
        let wrapped = std::fs::read_to_string(&path).unwrap();
        std::fs::remove_dir_all(&root).unwrap();
        assert!(usage.is_err_and(|e| e.starts_with("usage:")));
        assert_eq!((first, second), (Ok(true), Ok(true)));
        assert!(
            plain
                .as_ref()
                .is_err_and(|e| e.ends_with("no unwrapped example opens here")),
            "b is wrapped already: {plain:?}"
        );
        assert!(
            unreadable
                .as_ref()
                .is_err_and(|e| e.contains("base_odd.rs")),
            "a copies file that does not read is no new file: {unreadable:?}"
        );
        assert_eq!(odd, [0xff, 0xfe], "and stays as it was");
        assert!(
            copies.starts_with(
                "//! The examples of `enshwick-base` that name items above its crate,"
            ) && copies.contains("//! `a`:")
                && copies.ends_with(
                    "//! `b`:\n//!\n//! ```\n//! let b = enshwick::Load::new();\n//! ```\n"
                ),
            "{copies}"
        );
        assert_eq!(
            wrapped.matches(WRAPPER_OPEN).count(),
            2,
            "c stays unwrapped: {wrapped}"
        );
    }
}
