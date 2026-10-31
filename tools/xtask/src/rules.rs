//! The ast-grep rules over the repository, less the findings
//! `tools/ast-grep/known.txt` lists: code written before its rule, which
//! leaves the list when it moves to what the rule asks. A finding the list
//! does not name fails, and so does an entry nothing matches any more, so
//! the list only shrinks.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

/// The list of findings from before their rules, from the workspace root.
const KNOWN: &str = "tools/ast-grep/known.txt";

/// What names a finding in the list: its rule, its file and the first line
/// of the code it matched, trimmed, so the key holds while lines above it
/// move.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Key {
    rule: String,
    file: String,
    code: String,
}

/// A finding of `ast-grep scan`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Finding {
    key: Key,
    /// Its line, from 1.
    line: u64,
    message: String,
}

/// Runs the rules from `root` and reports what the list does not cover;
/// whether nothing was found beyond it.
pub(crate) fn scan(root: &Path) -> bool {
    let problems = match findings(root).and_then(|found| {
        let text = std::fs::read_to_string(root.join(KNOWN))
            .map_err(|error| format!("reading {KNOWN}: {error}"))?;
        Ok(compare(found, &known(&text)?))
    }) {
        Ok(problems) => problems,
        Err(error) => vec![error],
    };
    for problem in &problems {
        crate::say(problem);
    }
    if problems.is_empty() {
        crate::say("ok: ast-grep scan");
    }
    problems.is_empty()
}

/// The findings of every rule over the repository.
fn findings(root: &Path) -> Result<Vec<Finding>, String> {
    let output = Command::new("ast-grep")
        .args(["scan", "--json=stream"])
        .current_dir(root)
        .output()
        .map_err(|error| format!("running ast-grep: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let found = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(finding)
        .collect::<Result<Vec<_>, _>>()?;
    // A failed scan with nothing found failed before any rule ran.
    if !output.status.success() && found.is_empty() {
        return Err(format!(
            "ast-grep scan failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(found)
}

/// One line of `ast-grep scan --json=stream`.
fn finding(line: &str) -> Result<Finding, String> {
    let value: serde_json::Value =
        serde_json::from_str(line).map_err(|error| format!("reading ast-grep output: {error}"))?;
    let text = |field: &str| value[field].as_str().unwrap_or_default().to_owned();
    Ok(Finding {
        key: Key {
            rule: text("ruleId"),
            file: text("file").replace('\\', "/"),
            code: text("lines")
                .lines()
                .next()
                .unwrap_or_default()
                .trim()
                .to_owned(),
        },
        line: value["range"]["start"]["line"].as_u64().unwrap_or_default() + 1,
        message: text("message"),
    })
}

/// The entries of the list `text`, each with its line: `<rule> <file>
/// <code>`, blank lines and `#` comments between them.
fn known(text: &str) -> Result<Vec<(usize, Key)>, String> {
    text.lines()
        .enumerate()
        .filter(|(_, line)| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(|(at, line)| {
            let mut words = line.trim().splitn(3, ' ');
            match (words.next(), words.next(), words.next()) {
                (Some(rule), Some(file), Some(code)) => Ok((
                    at + 1,
                    Key {
                        rule: rule.to_owned(),
                        file: file.to_owned(),
                        code: code.trim().to_owned(),
                    },
                )),
                _ => Err(format!("{KNOWN}:{}: not `<rule> <file> <code>`", at + 1)),
            }
        })
        .collect()
}

/// The findings `known` does not cover, then the entries of `known` that
/// match nothing, one line each. An entry covers one finding, so a second
/// copy of a known line is new.
fn compare(found: Vec<Finding>, known: &[(usize, Key)]) -> Vec<String> {
    let mut open: BTreeMap<&Key, Vec<usize>> = BTreeMap::new();
    for (line, key) in known {
        open.entry(key).or_default().push(*line);
    }
    let mut problems = Vec::new();
    for finding in found {
        if open.get_mut(&finding.key).and_then(Vec::pop).is_none() {
            problems.push(format!(
                "{}:{}: {}: {}\n    {}",
                finding.key.file, finding.line, finding.key.rule, finding.message, finding.key.code
            ));
        }
    }
    let mut stale: Vec<(usize, &Key)> = open
        .into_iter()
        .flat_map(|(key, lines)| lines.into_iter().map(move |line| (line, key)))
        .collect();
    stale.sort();
    problems.extend(stale.into_iter().map(|(line, key)| {
        format!(
            "{KNOWN}:{line}: {} in {} no longer matches; remove the entry",
            key.rule, key.file
        )
    }));
    problems
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = r#"{"text":"static X: Mutex<Vec<u8>> = Mutex::new(Vec::new());","range":{"start":{"line":4,"column":0}},"file":"crates/a/src/b.rs","lines":"static X: Mutex<Vec<u8>> = Mutex::new(Vec::new());\n","ruleId":"static-is-registry-fuse-or-state","message":"keep module state elsewhere"}"#;

    fn key(code: &str) -> Key {
        Key {
            rule: "static-is-registry-fuse-or-state".to_owned(),
            file: "crates/a/src/b.rs".to_owned(),
            code: code.to_owned(),
        }
    }

    #[test]
    fn a_finding_is_keyed_by_rule_file_and_its_first_line() {
        let found = finding(LINE).unwrap();
        assert_eq!(
            found.key,
            key("static X: Mutex<Vec<u8>> = Mutex::new(Vec::new());")
        );
        assert_eq!(found.line, 5);
    }

    #[test]
    fn the_list_skips_comments_and_blank_lines_and_keeps_line_numbers() {
        let text =
            "# why\n\nstatic-is-registry-fuse-or-state crates/a/src/b.rs static X: u8 = 0;\n";
        assert_eq!(known(text).unwrap(), [(3, key("static X: u8 = 0;"))]);
        assert!(known("only-a-rule\n").is_err());
    }

    #[test]
    fn a_listed_finding_passes_and_an_unlisted_one_fails() {
        let found = finding(LINE).unwrap();
        let listed = [(1, found.key.clone())];
        assert!(compare(vec![found.clone()], &listed).is_empty());
        let problems = compare(vec![found], &[]);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("crates/a/src/b.rs:5: static-is-registry-fuse-or-state"));
    }

    #[test]
    fn an_entry_that_matches_nothing_fails() {
        let problems = compare(Vec::new(), &[(7, key("static Y: u8 = 0;"))]);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].starts_with("tools/ast-grep/known.txt:7:"));
    }

    #[test]
    fn one_entry_covers_one_copy_of_a_line() {
        let found = finding(LINE).unwrap();
        let listed = [(1, found.key.clone())];
        assert_eq!(compare(vec![found.clone(), found], &listed).len(), 1);
    }
}
