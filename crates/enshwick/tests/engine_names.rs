//! The API crate names engine types only through the loader's ids: an
//! engine name in its source fails here, except in the examples of the low
//! level calls that take an engine name on purpose.

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    /// The documented items whose examples may spell an engine name, by
    /// file under its crate's `src`, with the prefix of [`roots`], and item,
    /// each with its reason.
    const ALLOWED: &[(&str, &str, &str)] = &[
        ("base:ecs.rs", "ComponentType", COMPONENT),
        ("base:ecs.rs", "hash", COMPONENT),
        ("base:load.rs", "Loading", COMPONENT),
        ("inventory:effects.rs", "emit_raw", EVENT),
        ("base:game_events.rs", "EventType", EVENT),
        ("base:game_events.rs", "field", EVENT),
        ("base:game_events.rs", "hash", EVENT),
        ("base:game_events.rs", "inherited_field", EVENT),
        ("base:game_events.rs", "offset", EVENT),
        ("base:game_events.rs", "size", EVENT),
        ("load/build.rs", "field_offset", FIELD),
        ("load/build.rs", "type_size", FIELD),
        ("fake:test/base/hooks.rs", "field_offset", FAKE),
        ("world:world.rs", "WritableType", COMPONENT),
        ("world:world.rs", "set", COMPONENT),
        ("world:world.rs", "size", COMPONENT),
        ("world:world.rs", "update", COMPONENT),
        ("world:world/entities.rs", "entities", COMPONENT),
    ];

    const EVENT: &str = "`Load::game_event_type` takes any reflected event type by name";
    const FIELD: &str = "`Load::field_offset` and `type_size` read any reflected type";
    const COMPONENT: &str =
        "`ComponentType::resolve` and `WritableType::resolve` take any component";
    const FAKE: &str = "the fake host answers the low level names a mod asks for";

    /// Every Rust source file under `dir`.
    fn sources(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files.extend(sources(&path));
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                files.push(path);
            }
        }
        files
    }

    /// The item the doc comment on line `at` documents: the name after the
    /// keyword of the first line below that is neither a doc comment nor an
    /// attribute, or `module` for an inner doc comment.
    fn documented_item(lines: &[&str], at: usize) -> String {
        if lines[at].trim_start().starts_with("//!") {
            return "module".to_owned();
        }
        let line = lines[at..]
            .iter()
            .map(|line| line.trim_start())
            .find(|line| !line.starts_with("///") && !line.starts_with("#["))
            .unwrap_or_default();
        let words: Vec<&str> = line
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|word| !word.is_empty())
            .collect();
        let keywords = [
            "fn", "struct", "enum", "trait", "const", "static", "type", "mod", "unsafe",
        ];
        words
            .windows(2)
            .find(|pair| keywords.contains(&pair[0]) && !keywords.contains(&pair[1]))
            .map_or(line, |pair| pair[1])
            .to_owned()
    }

    /// The source folders of this crate and of every internal crate, each with
    /// the prefix its files take in [`ALLOWED`]: none for this crate and
    /// core, else the crate's folder name. `enshwick-typed` is generated from
    /// the reflection, whose names it carries on purpose.
    fn roots() -> Vec<(String, PathBuf)> {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        std::iter::once((String::new(), crate_dir.join("src")))
            .chain(
                crate::internal_crates::internal_crates()
                    .into_iter()
                    .filter(|(name, _)| name != "typed")
                    .map(|(name, src)| {
                        let prefix = if name == "core" {
                            String::new()
                        } else {
                            format!("{name}:")
                        };
                        (prefix, src)
                    }),
            )
            .collect()
    }

    #[test]
    fn no_engine_name_is_spelled_outside_the_low_level_examples() {
        let mut found = Vec::new();
        for (prefix, src) in roots() {
            scan(&prefix, &src, &mut found);
        }
        check(found);
    }

    /// Each documented item or code line under `src` that spells an engine
    /// name, by its file (after `prefix`) and item.
    fn scan(prefix: &str, src: &Path, found: &mut Vec<(String, String)>) {
        for path in sources(src) {
            let relative = path
                .strip_prefix(src)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            let file = format!("{prefix}{relative}");
            let text = std::fs::read_to_string(&path).unwrap();
            let lines: Vec<&str> = text.lines().collect();
            for (at, line) in lines.iter().enumerate() {
                if !line.contains("\"keen::") {
                    continue;
                }
                let trimmed = line.trim_start();
                let item = if trimmed.starts_with("///") || trimmed.starts_with("//!") {
                    documented_item(&lines, at)
                } else {
                    format!("code at line {}", at + 1)
                };
                found.push((file.clone(), item));
            }
        }
    }

    /// Fails on an engine name outside [`ALLOWED`] and on an allowed item
    /// that no longer spells one.
    fn check(mut found: Vec<(String, String)>) {
        found.sort();
        found.dedup();
        let unexpected: Vec<_> = found
            .iter()
            .filter(|(file, item)| {
                !ALLOWED
                    .iter()
                    .any(|&(f, i, _)| f == file.as_str() && i == item.as_str())
            })
            .collect();
        assert!(
            unexpected.is_empty(),
            "engine names outside the allowed examples; ask the loader by id: {unexpected:?}"
        );
        let stale: Vec<_> = ALLOWED
            .iter()
            .filter(|&&(f, i, _)| !found.iter().any(|(file, item)| file == f && item == i))
            .collect();
        assert!(stale.is_empty(), "allowed but no longer there: {stale:?}");
    }

    #[test]
    fn a_doc_comment_names_the_item_below_it() {
        let lines = [
            "/// Example.",
            "#[must_use]",
            "    pub const unsafe fn field_offset(&self) {}",
        ];
        assert_eq!(documented_item(&lines, 0), "field_offset");
        assert_eq!(documented_item(&["//! Module."], 0), "module");
    }
}
