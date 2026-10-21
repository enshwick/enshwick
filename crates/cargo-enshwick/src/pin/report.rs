//! The Markdown report a maintainer reviews.

use std::fmt::Write as _;

use super::row::{Line, Status};
use super::rows::hex;

/// One ported row in the report.
#[derive(Debug)]
pub(crate) struct Section {
    /// The source file, relative to the repository.
    pub(crate) file: String,
    /// The row's title: its name or side.
    pub(crate) title: String,
    /// One line per entry.
    pub(crate) lines: Vec<Line>,
    /// The new row's text, when it is not written into the file.
    pub(crate) text: Option<(String, &'static str)>,
}

/// The report for every section.
pub(crate) fn render(old: &str, new: &str, sections: &[Section]) -> String {
    let mut out = String::new();
    let all: Vec<&Line> = sections.iter().flat_map(|s| &s.lines).collect();
    let count = |status: Status| all.iter().filter(|l| l.status == status).count();
    let _w = writeln!(out, "# Pin report\n\nFrom build `{old}` to `{new}`.\n");
    let _w = writeln!(
        out,
        "{} entries: {} unchanged, {} moved, {} ambiguous, {} missing; constants {} kept, {} flagged, {} unverified.\n",
        all.len(),
        count(Status::Unchanged),
        count(Status::Moved),
        count(Status::Ambiguous),
        count(Status::Missing),
        count(Status::Kept),
        count(Status::Flagged),
        count(Status::Unverified),
    );
    let human: Vec<&Line> = all
        .iter()
        .copied()
        .filter(|l| l.status.needs_human())
        .collect();
    if human.is_empty() {
        let _w = writeln!(out, "Nothing needs a human.\n");
    } else {
        let _w = writeln!(out, "## For a human\n");
        for line in human {
            let _w = writeln!(
                out,
                "- {} `{}` {}: {}",
                line.kind, line.id, line.status, line.note
            );
        }
        out.push('\n');
    }
    for section in sections {
        let _w = writeln!(out, "## {}: {}\n", section.file, section.title);
        let _w = writeln!(
            out,
            "| Kind | Id | Old | New | Status | Confidence | Note |\n| --- | --- | --- | --- | --- | --- | --- |"
        );
        for line in &section.lines {
            let _w = writeln!(
                out,
                "| {} | `{}` | {} | {} | {} | {} | {} |",
                line.kind,
                line.id,
                hex(line.old),
                line.new.map_or_else(|| "-".to_owned(), hex),
                line.status,
                line.confidence.as_deref().unwrap_or("-"),
                line.note.replace('|', "/"),
            );
        }
        out.push('\n');
        if let Some((text, language)) = &section.text {
            let _w = writeln!(out, "New row:\n\n```{language}\n{text}\n```\n");
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pin::row::Kind;

    #[test]
    fn the_report_lists_what_needs_a_human_first() {
        let line = |id: &str, status| Line {
            kind: Kind::Symbol,
            id: id.to_owned(),
            old: 0x10,
            new: None,
            status,
            confidence: None,
            note: "why | not".to_owned(),
        };
        let report = render(
            "a/1",
            "b/2",
            &[Section {
                file: "x.rs".to_owned(),
                title: "server".to_owned(),
                lines: vec![line("a", Status::Unchanged), line("b", Status::Ambiguous)],
                text: Some(("{ }".to_owned(), "rust")),
            }],
        );
        assert!(report.contains("2 entries: 1 unchanged, 0 moved, 1 ambiguous"));
        assert!(report.contains("## For a human\n\n- symbol `b` ambiguous: why | not\n"));
        assert!(report.contains("| symbol | `a` | 0x10 | - | unchanged | - | why / not |"));
        assert!(report.contains("```rust\n{ }\n```"));
    }
}
