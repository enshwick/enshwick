//! Ports one build row: its checks through reflection, its symbols through
//! signatures, and its constants through the code that uses them.

use std::collections::HashMap;
use std::fmt;

use enshwick_loader_builds::Table;

use super::anchor::{self, Anchor};
use super::exe::Exe;
use super::port::{Outcome, Ported, Porter};
use super::rows::{Edit, Entry, Row, hex};

/// Constants below this are too common in code to sample.
const MIN_SAMPLED: u64 = 0x10;

/// What kind of entry a report line is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Kind {
    /// A system whose reflected entry must match.
    Check,
    /// A pinned address.
    Symbol,
    /// A pinned constant.
    Constant,
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Check => "check",
            Self::Symbol => "symbol",
            Self::Constant => "constant",
        })
    }
}

/// The verdict on one entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Status {
    /// At the same address.
    Unchanged,
    /// At a new address.
    Moved,
    /// More than one candidate.
    Ambiguous,
    /// No candidate.
    Missing,
    /// A constant whose sampled uses are unchanged.
    Kept,
    /// A constant whose uses changed.
    Flagged,
    /// A constant with no use to sample.
    Unverified,
    /// A constant read from the ported instruction that uses it.
    Read,
}

impl Status {
    /// Whether a human must decide before the row is used.
    pub(crate) fn needs_human(self) -> bool {
        matches!(self, Self::Ambiguous | Self::Missing | Self::Flagged)
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unchanged => "unchanged",
            Self::Moved => "moved",
            Self::Ambiguous => "ambiguous",
            Self::Missing => "missing",
            Self::Kept => "kept",
            Self::Flagged => "flagged",
            Self::Unverified => "unverified",
            Self::Read => "read",
        })
    }
}

/// One line of the report.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Line {
    /// What kind of entry.
    pub(crate) kind: Kind,
    /// Its id.
    pub(crate) id: String,
    /// The old value.
    pub(crate) old: u64,
    /// The new value, when there is one.
    pub(crate) new: Option<u64>,
    /// The verdict.
    pub(crate) status: Status,
    /// How sure, for a found address.
    pub(crate) confidence: Option<String>,
    /// How it was decided.
    pub(crate) note: String,
}

/// A row, ported: the edits for its new text and the report.
#[derive(Debug)]
pub(crate) struct PortedRow<'r> {
    /// How each entry changes.
    pub(crate) edits: Vec<(&'r Entry, Edit)>,
    /// One line per entry.
    pub(crate) lines: Vec<Line>,
}

/// Ports `row`, made for `old`, to `new`.
pub(crate) fn port_row<'r>(row: &'r Row, old: &Exe<'_>, new: &Exe<'_>) -> PortedRow<'r> {
    let mut edits = Vec::new();
    let mut lines = Vec::new();
    for entry in &row.checks {
        let (edit, line) = check(entry, old, new);
        edits.push((entry, edit));
        lines.push(line);
    }
    let finder = Porter::new(old, new);
    let ported: Vec<(&Entry, Ported)> = row
        .symbols
        .iter()
        .map(|entry| (entry, finder.symbol(entry.value)))
        .collect();
    for (entry, port) in &ported {
        let (edit, line) = symbol(entry, port);
        edits.push((*entry, edit));
        lines.push(line);
    }
    for entry in &row.constants {
        let (edit, line) = constant(entry, &ported, old, new);
        edits.push((entry, edit));
        lines.push(line);
    }
    PortedRow { edits, lines }
}

fn moved(old: u64, new: u64) -> Status {
    if old == new {
        Status::Unchanged
    } else {
        Status::Moved
    }
}

fn check(entry: &Entry, old: &Exe<'_>, new: &Exe<'_>) -> (Edit, Line) {
    let mut line = Line {
        kind: Kind::Check,
        id: entry.id.clone(),
        old: entry.value,
        new: None,
        status: Status::Missing,
        confidence: None,
        note: String::new(),
    };
    if old.system_entry(&entry.id) != Some(entry.value) {
        "the old row does not match the old build's reflection".clone_into(&mut line.note);
    } else if let Some(rva) = new.system_entry(&entry.id) {
        line.new = Some(rva);
        line.status = moved(entry.value, rva);
        line.confidence = Some("reflection".to_owned());
        "the system's reflected entry".clone_into(&mut line.note);
        return (Edit::Set(rva), line);
    } else {
        "the new build reflects no such system".clone_into(&mut line.note);
    }
    let note = format!(
        "{} missing: {}; was {}",
        entry.id,
        line.note,
        hex(entry.value)
    );
    (Edit::Drop(note), line)
}

fn symbol(entry: &Entry, port: &Ported) -> (Edit, Line) {
    let mut line = Line {
        kind: Kind::Symbol,
        id: entry.id.clone(),
        old: entry.value,
        new: None,
        status: Status::Missing,
        confidence: None,
        note: String::new(),
    };
    match &port.outcome {
        Outcome::Found {
            rva,
            confidence,
            how,
        } => {
            line.new = Some(*rva);
            line.status = moved(entry.value, *rva);
            line.confidence = Some(confidence.to_string());
            line.note.clone_from(how);
            return (Edit::Set(*rva), line);
        }
        Outcome::Ambiguous(why) => {
            line.status = Status::Ambiguous;
            line.note.clone_from(why);
        }
        Outcome::Missing(why) => line.note.clone_from(why),
    }
    let note = format!(
        "{} {}: {}; was {}",
        entry.id,
        line.status,
        line.note,
        hex(entry.value)
    );
    (Edit::Drop(note), line)
}

fn constant(
    entry: &Entry,
    ported: &[(&Entry, Ported)],
    old: &Exe<'_>,
    new: &Exe<'_>,
) -> (Edit, Line) {
    let value = entry.value;
    let (status, note) = if value > u64::from(u32::MAX) {
        code_bytes(value, ported, old, new)
    } else if value < MIN_SAMPLED {
        (Status::Unverified, "too small to sample in code".to_owned())
    } else {
        sampled(value, ported)
    };
    let edit = if status == Status::Flagged {
        Edit::Note(format!("{} flagged: {note}", entry.id))
    } else {
        Edit::Keep
    };
    let line = Line {
        kind: Kind::Constant,
        id: entry.id.clone(),
        old: value,
        new: Some(value),
        status,
        confidence: None,
        note,
    };
    (edit, line)
}

/// A constant that holds the first code bytes of one of the row's symbols.
fn code_bytes(
    value: u64,
    ported: &[(&Entry, Ported)],
    old: &Exe<'_>,
    new: &Exe<'_>,
) -> (Status, String) {
    let read = |exe: &Exe<'_>, rva: u64| {
        exe.code(rva, 8)
            .and_then(|b| b.try_into().ok())
            .map(u64::from_le_bytes)
    };
    for (entry, port) in ported {
        if read(old, entry.value) != Some(value) {
            continue;
        }
        let Some(rva) = port.outcome.rva() else {
            return (
                Status::Flagged,
                format!("the code bytes at {}, which was not found", entry.id),
            );
        };
        return match read(new, rva) {
            Some(now) if now == value => (
                Status::Kept,
                format!("the code bytes at {} are unchanged", entry.id),
            ),
            now => (
                Status::Flagged,
                format!(
                    "the code bytes at {} now read {}",
                    entry.id,
                    now.map_or_else(|| "nothing".to_owned(), hex)
                ),
            ),
        };
    }
    (
        Status::Unverified,
        "no symbol of this row starts with these bytes".to_owned(),
    )
}

/// A constant used as a displacement or immediate in the row's code.
fn sampled(value: u64, ported: &[(&Entry, Ported)]) -> (Status, String) {
    let (mut kept, mut changed) = (0usize, 0usize);
    for (_, port) in ported {
        let Some((start, masked)) = &port.old_code else {
            continue;
        };
        for insn in masked.insns.iter().filter(|i| i.uses(value)) {
            let site = start + insn.offset as u64;
            let proven = port.outcome.rva().is_some()
                && port
                    .proven
                    .as_ref()
                    .is_some_and(|r| r.start <= site && site + insn.len as u64 <= r.end);
            if proven {
                kept += 1;
            } else {
                changed += 1;
            }
        }
    }
    if kept > 0 {
        let also = if changed > 0 {
            format!(", {changed} in code not proven unchanged")
        } else {
            String::new()
        };
        (Status::Kept, format!("{kept} uses in matched code{also}"))
    } else if changed > 0 {
        (
            Status::Flagged,
            format!("{changed} uses, none in code proven unchanged"),
        )
    } else {
        (
            Status::Unverified,
            "no use in this row's functions".to_owned(),
        )
    }
}

/// Finds the anchors of `table`'s entries on `new`, by their patterns or by
/// porting their instructions from `old`, and settles those entries of
/// `ported` by them: a constant takes the value its ported
/// instruction carries, a symbol the target of its ported reference. The
/// anchors in `new`, for the new table's patterns.
pub(crate) fn port_anchors<'r>(
    row: &'r Row,
    ported: &mut PortedRow<'r>,
    table: &Table,
    old: &Exe<'_>,
    new: &Exe<'_>,
) -> HashMap<String, Anchor> {
    let finder = Porter::new(old, new);
    // The entry's own pattern finds its instruction on the new build while
    // the code around it holds; else the instruction is ported.
    let in_new = anchor::of_table(table, new);
    let mut out = HashMap::new();
    for (id, anchor) in anchor::of_table(table, old) {
        let by_port = || {
            let site = anchor.site().and_then(|s| finder.symbol(s).outcome.rva())?;
            Some(anchor.moved_to(site))
        };
        let Some(here) = in_new.get(&id).cloned().or_else(by_port) else {
            continue;
        };
        let Some(site) = here.site() else {
            continue;
        };
        let at = hex(site);
        let settled = match &here {
            Anchor::Read(..) => anchor::read(new, &here).map(|value| {
                let entry = row.constants.iter().find(|e| e.id == id);
                let status = if entry.is_some_and(|e| e.value == value) {
                    Status::Kept
                } else {
                    Status::Read
                };
                (
                    value,
                    status,
                    None,
                    format!("read from the code that uses it, at {at}"),
                )
            }),
            Anchor::Through(site) => anchor::target(new, *site).map(|rva| {
                let entry = row.symbols.iter().find(|e| e.id == id);
                let status = moved(entry.map_or(rva, |e| e.value), rva);
                (
                    rva,
                    status,
                    Some("medium".to_owned()),
                    format!("what the code at {} reaches", hex(*site)),
                )
            }),
            Anchor::Confirm(_) | Anchor::Own => None,
        };
        let Some((value, status, confidence, note)) = settled else {
            continue;
        };
        let Some(index) = ported.lines.iter().position(|l| l.id == id) else {
            continue;
        };
        let line = &mut ported.lines[index];
        if let (Kind::Symbol, Some(own)) = (line.kind, line.new)
            && own != value
        {
            line.status = Status::Ambiguous;
            line.note = format!("its own code is at {}, {note} {}", hex(own), hex(value));
            let drop = format!("{id} ambiguous: {}; was {}", line.note, hex(line.old));
            if let Some(edit) = ported.edits.iter_mut().find(|(e, _)| e.id == id) {
                edit.1 = Edit::Drop(drop);
            }
            continue;
        }
        if line.kind == Kind::Symbol && line.new.is_some() {
            out.insert(id, here);
            continue;
        }
        line.new = Some(value);
        line.status = status;
        line.confidence = confidence;
        line.note = note;
        if let Some(edit) = ported.edits.iter_mut().find(|(e, _)| e.id == id) {
            edit.1 = Edit::Set(value);
        }
        out.insert(id, here);
    }
    out
}

/// Settles the constants of `ported` whose pattern in `table` only
/// confirms the value: kept where it still matches once in `new`.
pub(crate) fn confirm_by_patterns(ported: &mut PortedRow<'_>, table: &Table, new: &Exe<'_>) {
    let (code, _) = new.text();
    for constant in table.constants() {
        let (Some(pattern), None) = (constant.pattern(), constant.operand()) else {
            continue;
        };
        if pattern.find_unique(code).is_err() {
            continue;
        }
        if let Some(line) = ported
            .lines
            .iter_mut()
            .find(|l| l.kind == Kind::Constant && l.id == constant.id())
        {
            line.status = Status::Kept;
            "the code that confirms it is unchanged".clone_into(&mut line.note);
        }
    }
}
