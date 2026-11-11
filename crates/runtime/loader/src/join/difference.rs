//! What differs between the mods a server's world runs and a client's: the
//! list the mismatch screen shows after a refusal or a kick.
//!
//! The server's side is the `id@version,...` list it advertises, peer data
//! the list never trusts: [`entries`](super::entries) drops what is no mod id
//! or version, a repeated id counts once, and the list holds at most
//! [`MAX_LINES`] lines, counting the rest.

use std::collections::BTreeMap;

use super::entries;

/// The most lines the difference keeps over its three kinds; the rest are
/// counted in [`Difference::left_out`].
pub const MAX_LINES: usize = 512;

/// One mod that differs.
///
/// # Examples
///
/// See [`difference`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The mod id.
    pub id: String,
    /// The version the server's world runs; `None` for a mod it does not.
    pub needed: Option<String>,
    /// The client's version; `None` for a mod it does not have.
    pub installed: Option<String>,
    /// The client has the mod, turned off until the game restarts.
    pub off: bool,
}

/// The mods that differ, by kind, each sorted by id.
///
/// # Examples
///
/// See [`difference`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Difference {
    /// The server's mods the client lacks, or has turned off.
    pub missing: Vec<Line>,
    /// The server's mods the client runs in another version, or another
    /// pack build.
    pub mismatch: Vec<Line>,
    /// The client's mods the world does not run: turned off for that world,
    /// never the cause of a refusal.
    pub extra: Vec<Line>,
    /// Lines past [`MAX_LINES`], not kept.
    pub left_out: usize,
}

impl Difference {
    /// Whether the two sides run the same mods.
    ///
    /// # Examples
    ///
    /// ```
    /// assert!(enshwick_loader::join::Difference::default().is_empty());
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.missing.is_empty() && self.mismatch.is_empty() && self.extra.is_empty()
    }

    /// Whether the difference keeps the client out: a mod of the server's
    /// it lacks or runs in another version.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::join::difference;
    ///
    /// assert!(!difference("", "you.lanterns@1.0.0", "", true).refuses());
    /// assert!(difference("you.lanterns@1.0.0", "", "", true).refuses());
    /// ```
    #[must_use]
    pub fn refuses(&self) -> bool {
        !self.missing.is_empty() || !self.mismatch.is_empty()
    }

    /// The screen's rows in the player's language: a heading with its count
    /// per kind that has mods, then a line per mod, and the count of the
    /// lines left out. Ids and versions stay as the mods give them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::join::{Row, difference};
    ///
    /// let rows = difference("you.lanterns@0.3.0", "", "", true).rows();
    /// assert_eq!(rows[0], Row::Heading("Missing: 1".into()));
    /// assert_eq!(rows[1], Row::Line("you.lanterns: needs 0.3.0, you have none".into()));
    /// ```
    #[must_use]
    pub fn rows(&self) -> Vec<Row> {
        let none = crate::texts::fill("join.screen.none", &[]);
        let line = |line: &Line| {
            let needed = line.needed.as_deref().unwrap_or_default();
            let installed = line.installed.as_deref().unwrap_or(&none);
            let args = [
                ("id", line.id.as_str()),
                ("needed", needed),
                ("installed", installed),
            ];
            let key = if line.off {
                "join.screen.off"
            } else {
                "join.screen.line"
            };
            Row::Line(crate::texts::fill(key, &args))
        };
        let mut rows = Vec::new();
        for (key, lines) in [
            ("join.screen.missing", &self.missing),
            ("join.screen.version", &self.mismatch),
        ] {
            if !lines.is_empty() {
                let count = lines.len().to_string();
                rows.push(Row::Heading(crate::texts::fill(key, &[("count", &count)])));
                rows.extend(lines.iter().map(line));
            }
        }
        if !self.extra.is_empty() {
            let count = self.extra.len().to_string();
            rows.push(Row::Heading(crate::texts::fill(
                "join.screen.extra",
                &[("count", &count)],
            )));
            rows.extend(self.extra.iter().map(|line| {
                let installed = line.installed.as_deref().unwrap_or_default();
                Row::Quiet(format!("{} {installed}", line.id))
            }));
        }
        if self.left_out > 0 {
            let count = self.left_out.to_string();
            rows.push(Row::Quiet(crate::texts::fill(
                "join.refused.more",
                &[("count", &count)],
            )));
        }
        rows
    }
}

/// One row of the mismatch screen, by how it shows.
///
/// # Examples
///
/// See [`Difference::rows`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Row {
    /// A kind's heading with its count.
    Heading(String),
    /// A mod that keeps the client out.
    Line(String),
    /// A line for information: an extra mod, or the count left out.
    Quiet(String),
}

/// The mods that differ between a server's world and this client.
///
/// `server` lists the mods of side both the server's world runs and `ours`
/// the client's running ones, both `id@version,...`; `off` lists the
/// client's mods of side both turned off until the game restarts. A server
/// list that is not `complete` holds only its first mods, so the client's
/// other mods are not counted as extra.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::difference;
///
/// let found = difference("you.lanterns@0.3.0,you.zones@1.0.0", "you.zones@0.9.0,you.camera@1.0.0", "", true);
/// assert_eq!(found.missing[0].id, "you.lanterns");
/// assert_eq!(found.missing[0].installed, None);
/// assert_eq!(found.mismatch[0].needed.as_deref(), Some("1.0.0"));
/// assert_eq!(found.mismatch[0].installed.as_deref(), Some("0.9.0"));
/// assert_eq!(found.extra[0].id, "you.camera");
/// ```
#[must_use]
pub fn difference(server: &str, ours: &str, off: &str, complete: bool) -> Difference {
    // A repeated id of a hostile list counts once, by its first entry.
    let mut theirs: BTreeMap<&str, &str> = BTreeMap::new();
    for (id, version) in entries(server) {
        theirs.entry(id).or_insert(version);
    }
    let ours: BTreeMap<&str, &str> = entries(ours).collect();
    let off: BTreeMap<&str, &str> = entries(off).collect();
    let mut lines = 0;
    let mut keep = |kind: &mut Vec<Line>, line: Line| {
        lines += 1;
        if lines <= MAX_LINES {
            kind.push(line);
        }
    };
    let mut missing = Vec::new();
    let mut mismatch = Vec::new();
    let mut extra = Vec::new();
    for (&id, &needed) in &theirs {
        let line = |installed: Option<&str>, off: bool| Line {
            id: id.to_owned(),
            needed: Some(needed.to_owned()),
            installed: installed.map(str::to_owned),
            off,
        };
        match (ours.get(id), off.get(id)) {
            (Some(&mine), _) if mine == needed => {}
            (Some(&mine), _) => keep(&mut mismatch, line(Some(mine), false)),
            (None, Some(&mine)) => keep(&mut missing, line(Some(mine), true)),
            (None, None) => keep(&mut missing, line(None, false)),
        }
    }
    if complete {
        for (&id, &mine) in &ours {
            if !theirs.contains_key(id) {
                let line = Line {
                    id: id.to_owned(),
                    needed: None,
                    installed: Some(mine.to_owned()),
                    off: false,
                };
                keep(&mut extra, line);
            }
        }
    }
    Difference {
        missing,
        mismatch,
        extra,
        left_out: lines.saturating_sub(MAX_LINES),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn ids(lines: &[Line]) -> Vec<&str> {
        lines.iter().map(|line| line.id.as_str()).collect()
    }

    #[test]
    fn each_kind_names_the_id_the_needed_and_the_installed_version() {
        let found = difference(
            "you.lanterns@0.3.0,you.zones@1.0.0,you.post@1.0.0+0a1b2c3d",
            "you.zones@0.9.0,you.post@1.0.0+ffffffff,you.camera@1.0.0",
            "",
            true,
        );
        assert_eq!(
            found.missing,
            [Line {
                id: "you.lanterns".into(),
                needed: Some("0.3.0".into()),
                installed: None,
                off: false,
            }]
        );
        assert_eq!(ids(&found.mismatch), ["you.post", "you.zones"]);
        assert_eq!(
            found.mismatch[0].installed.as_deref(),
            Some("1.0.0+ffffffff"),
            "another pack build of one version differs"
        );
        assert_eq!(
            found.extra,
            [Line {
                id: "you.camera".into(),
                needed: None,
                installed: Some("1.0.0".into()),
                off: false,
            }]
        );
        assert!(found.refuses());
        assert_eq!(found.left_out, 0);
    }

    #[test]
    fn the_same_mods_make_no_difference_and_extra_mods_alone_refuse_nothing() {
        let same = difference("a.b@1.0.0", "a.b@1.0.0", "", true);
        assert!(same.is_empty() && !same.refuses());
        let extra = difference("", "a.b@1.0.0", "", true);
        assert_eq!(ids(&extra.extra), ["a.b"]);
        assert!(!extra.refuses(), "a vanilla server lets a modded client in");
        assert!(difference("", "", "", true).is_empty());
    }

    #[test]
    fn a_mod_off_until_a_restart_is_missing_with_its_version() {
        let found = difference("a.b@1.0.0", "", "a.b@1.0.0", true);
        assert_eq!(
            found.missing,
            [Line {
                id: "a.b".into(),
                needed: Some("1.0.0".into()),
                installed: Some("1.0.0".into()),
                off: true,
            }]
        );
    }

    #[test]
    fn a_cut_server_list_counts_no_extra_mod() {
        let found = difference("a.b@1.0.0", "a.b@1.0.0,c.d@1.0.0", "", false);
        assert!(found.is_empty());
    }

    #[test]
    fn the_rows_give_a_heading_per_kind_and_every_mod_by_id_and_versions() {
        let found = difference(
            "you.lanterns@0.3.0,you.zones@1.0.0,you.post@1.0.0",
            "you.zones@0.9.0,you.camera@1.0.0",
            "you.post@1.0.0",
            true,
        );
        assert_eq!(
            found.rows(),
            [
                Row::Heading("Missing: 2".into()),
                Row::Line("you.lanterns: needs 0.3.0, you have none".into()),
                Row::Line(
                    "you.post: needs 1.0.0, you have 1.0.0, off until you restart the game".into()
                ),
                Row::Heading("In another version: 1".into()),
                Row::Line("you.zones: needs 1.0.0, you have 0.9.0".into()),
                Row::Heading("Off for this server, which does not run them: 1".into()),
                Row::Quiet("you.camera 1.0.0".into()),
            ]
        );
        let all = format!("{:?}", found.rows()).to_lowercase();
        assert!(!all.contains("download") && !all.contains("http"), "{all}");
        let many: Vec<String> = (0..MAX_LINES + 3)
            .map(|i| format!("you.mod-{i:04}@1.0.0"))
            .collect();
        let rows = difference(&many.join(","), "", "", true).rows();
        assert_eq!(rows.last(), Some(&Row::Quiet("And 3 more.".into())));
    }

    #[test]
    fn two_hundred_mods_are_all_listed() {
        let server: Vec<String> = (0..200).map(|i| format!("you.mod-{i:03}@1.0.0")).collect();
        let found = difference(&server.join(","), "", "", true);
        assert_eq!(found.missing.len(), 200);
        assert_eq!(found.left_out, 0);
    }

    #[test]
    fn a_hostile_list_is_bounded_and_keeps_only_ids_and_versions() {
        let mut server: Vec<String> = (0..2000).map(|i| format!("you.mod-{i:04}@1.0.0")).collect();
        server.push(format!("you.long@{}", "1".repeat(500)));
        server.push("Not An Id@1.0.0".into());
        server.push("you.text@1.0.0\nInstall from evil.example".into());
        server.push("you.mod-0000@9.9.9".into());
        let found = difference(&server.join(","), "", "", true);
        assert_eq!(found.missing.len(), MAX_LINES);
        assert_eq!(found.left_out, 2000 - MAX_LINES);
        assert!(
            found
                .missing
                .iter()
                .all(|line| line.id.starts_with("you.mod-"))
        );
        assert_eq!(
            found.missing[0].needed.as_deref(),
            Some("1.0.0"),
            "the first entry counts"
        );
    }

    proptest! {
        #[test]
        fn any_peer_text_gives_a_bounded_list_of_valid_ids(server in ".{0,4000}", ours in "[a-z.@0-9,-]{0,200}") {
            let found = difference(&server, &ours, "", true);
            let lines = found.missing.len() + found.mismatch.len() + found.extra.len();
            prop_assert!(lines <= MAX_LINES);
            for line in found.missing.iter().chain(&found.mismatch).chain(&found.extra) {
                prop_assert!(enshwick_core::abi::is_valid_old_form_id(&line.id));
                for version in line.needed.iter().chain(&line.installed) {
                    prop_assert!(version.len() <= 64);
                    prop_assert!(version.bytes().all(|b| b.is_ascii_alphanumeric() || b".+-".contains(&b)));
                }
            }
        }
    }
}
