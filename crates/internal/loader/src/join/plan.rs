//! Comparing the server's mod set with the client's: what turns off, what
//! keeps the client out.

use std::collections::BTreeMap;

use crate::admission::Need;
use crate::texts;

use super::Names;

/// The longest version a server may advertise, in bytes.
const MAX_VERSION: usize = 64;

/// The `(id, version)` entries of an `id@version,...` list, without an id
/// that is no mod id or a version that is no version: a server writes the
/// list, and its ids and versions reach the player's notes and the mods.
pub(crate) fn entries(list: &str) -> impl Iterator<Item = (&str, &str)> {
    list.split(',')
        .filter_map(|entry| entry.split_once('@'))
        .filter(|&(id, version)| {
            enshwick::abi::is_valid_old_form_id(id)
                && !version.is_empty()
                && version.len() <= MAX_VERSION
                && version
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-'))
        })
}

/// The name players read for mod `id`, or the id when `names` has none.
pub(super) fn label(names: &Names, id: &str) -> String {
    enshwick::mods::label(names.get(id).map_or("", String::as_str), id)
}

/// What the client does when it joins.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::plan;
///
/// let plan = plan("e5db5dca", "you.trading-post@1.0.0", Some(("989b6a71", "you.trading-post@2.0.0")), true);
/// assert!(!plan.joins());
/// assert_eq!(plan.missing[0].yours.as_deref(), Some("1.0.0"));
/// ```
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// The client's mods the server does not run: turned off, with a note.
    pub turn_off: Vec<String>,
    /// The server's mods the client lacks or runs in another version: the
    /// client does not join.
    pub missing: Vec<Need>,
    /// The mods match but the loader versions differ.
    pub other_loader: bool,
}

impl Plan {
    /// Whether the client joins: the server runs no mod it lacks.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::join::Plan;
    ///
    /// assert!(Plan::default().joins());
    /// ```
    #[must_use]
    pub fn joins(&self) -> bool {
        self.missing.is_empty()
    }

    /// The client's mods to turn off now: none when it stays out, since
    /// it never reaches the server that lacks them.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::join::Plan;
    ///
    /// let plan = Plan { turn_off: vec!["you.trading-post".to_owned()], ..Plan::default() };
    /// assert_eq!(plan.turned_off(), ["you.trading-post"]);
    /// ```
    #[must_use]
    pub fn turned_off(&self) -> &[String] {
        if self.joins() { &self.turn_off } else { &[] }
    }

    /// The note for a player who joins, if any, naming mods by `names`; a
    /// client kept out gets the [`refusal`] dialog instead.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::join::{Names, Plan};
    ///
    /// let plan = Plan { turn_off: vec!["you.trading-post".to_owned()], ..Plan::default() };
    /// assert!(plan.note(&Names::new()).is_some_and(|note| note.ends_with("you.trading-post")));
    /// assert_eq!(Plan::default().note(&Names::new()), None);
    /// ```
    #[must_use]
    pub fn note(&self, names: &Names) -> Option<String> {
        if !self.joins() {
            return None;
        }
        if !self.turn_off.is_empty() {
            let lines: Vec<String> = self.turn_off.iter().map(|id| label(names, id)).collect();
            return Some(format!(
                "{}\n\n{}",
                texts::fill("join.note.off", &[]),
                lines.join("\n")
            ));
        }
        self.other_loader
            .then(|| texts::fill("join.note.loader", &[]))
    }
}

/// The plan for joining a server that advertises `server` (fingerprint and mod
/// list), or no Enshwick at all.
///
/// A list that is not `complete` holds only the first of the server's mods, so
/// a mod of ours it does not name may still run there and stays on.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::plan;
///
/// let plan = plan("e5db5dca", "you.trading-post@1.0.0,camera@1.0.0", Some(("989b6a71", "you.trading-post@1.0.0")), true);
/// assert!(plan.joins());
/// assert_eq!(plan.turned_off(), ["camera"]);
/// ```
#[must_use]
pub fn plan(ours: &str, ours_mods: &str, server: Option<(&str, &str)>, complete: bool) -> Plan {
    let ids = |list: &str| -> Vec<String> { entries(list).map(|(id, _)| id.to_owned()).collect() };
    match server {
        Some((fingerprint, _)) if fingerprint == ours => Plan::default(),
        None => Plan {
            turn_off: ids(ours_mods),
            ..Plan::default()
        },
        Some((_, mods)) => {
            let theirs = ids(mods);
            let turn_off: Vec<String> = ids(ours_mods)
                .into_iter()
                .filter(|id| complete && !theirs.contains(id))
                .collect();
            let yours: BTreeMap<&str, &str> = entries(ours_mods).collect();
            let missing: Vec<Need> = wanted(mods, ours_mods)
                .into_iter()
                .map(|(id, version)| Need {
                    yours: yours.get(id.as_str()).map(|&v| v.to_owned()),
                    id,
                    version,
                })
                .collect();
            let other_loader = complete && missing.is_empty() && turn_off.is_empty();
            Plan {
                turn_off,
                missing,
                other_loader,
            }
        }
    }
}

/// One entry per mod whose version differs between two `id@version,...`
/// lists: its id, whether the server runs it, and a line naming it by
/// `names` and both versions.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::{Names, differences};
///
/// let found = differences("you.trading-post@2.0.0", "you.trading-post@1.0.0", &Names::new());
/// assert_eq!(found, [("you.trading-post".to_owned(), true, "you.trading-post: server 2.0.0, yours 1.0.0".to_owned())]);
/// ```
#[must_use]
pub fn differences(server: &str, ours: &str, names: &Names) -> Vec<(String, bool, String)> {
    let parse = |list: &str| -> BTreeMap<String, String> {
        entries(list)
            .map(|(id, version)| (id.to_owned(), version.to_owned()))
            .collect()
    };
    let (server, ours) = (parse(server), parse(ours));
    let mut ids: Vec<&String> = server.keys().chain(ours.keys()).collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .filter_map(|id| {
            let name = label(names, id);
            let line = match (server.get(id), ours.get(id)) {
                (Some(theirs), Some(mine)) if theirs != mine => {
                    format!("{name}: server {theirs}, yours {mine}")
                }
                (Some(theirs), None) => format!("{name}: server {theirs}, not installed"),
                (None, Some(mine)) => format!("{name}: yours {mine}, not on the server"),
                _ => return None,
            };
            Some((id.clone(), server.contains_key(id), line))
        })
        .collect()
}

/// The server's mods, `(id, version)`, that the client lacks or runs in
/// another version, from two `id@version,...` lists.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::wanted;
///
/// let wanted = wanted("you.trading-post@2.0.0,camera@1.0.0", "camera@1.0.0");
/// assert_eq!(wanted, [("you.trading-post".to_owned(), "2.0.0".to_owned())]);
/// ```
#[must_use]
pub fn wanted(server: &str, ours: &str) -> Vec<(String, String)> {
    let ours: BTreeMap<&str, &str> = entries(ours).collect();
    entries(server)
        .filter(|(id, version)| ours.get(id) != Some(version))
        .map(|(id, version)| (id.to_owned(), version.to_owned()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mods_the_server_lacks_are_turned_off_and_only_the_rest_warn() {
        let names = Names::from([("c".to_owned(), "Cats".to_owned())]);
        assert_eq!(
            plan("ab", "a@1,b@1", Some(("ab", "a@1,b@1")), true),
            Plan::default()
        );
        let none = plan("ab", "a@1,b@1", None, true);
        assert_eq!(none.turn_off, ["a", "b"]);
        assert!(none.note(&names).is_some());
        let extra = plan("ab", "a@1,b@1", Some(("cd", "a@1")), true);
        assert_eq!(extra.turn_off, ["b"]);
        assert!(extra.missing.is_empty());
        let missing = plan("ab", "a@1", Some(("cd", "a@2,c@1")), true);
        assert!(missing.turn_off.is_empty());
        assert_eq!(
            missing.missing,
            [
                Need {
                    id: "a".to_owned(),
                    version: "2".to_owned(),
                    yours: Some("1".to_owned()),
                },
                Need {
                    id: "c".to_owned(),
                    version: "1".to_owned(),
                    yours: None,
                },
            ]
        );
        assert_eq!(missing.note(&names), None, "a refusal is no join note");
        assert!(plan("ab", "a@1", Some(("cd", "a@1")), true).other_loader);
    }

    #[test]
    fn a_client_kept_out_turns_nothing_off() {
        let both = plan("ab", "a@1,b@1", Some(("cd", "a@2")), true);
        assert_eq!(both.turn_off, ["b"]);
        assert!(!both.joins());
        assert!(both.turned_off().is_empty());
        let extra = plan("ab", "a@1,b@1", Some(("cd", "a@1")), true);
        assert!(extra.joins());
        assert_eq!(extra.turned_off(), ["b"]);
    }

    #[test]
    fn a_mod_that_is_off_on_the_client_keeps_it_out() {
        // Server A lacked count, so it turned off; server B runs it.
        let plan = plan("x", "", Some(("y", "count@0.2.0")), true);
        assert!(!plan.joins());
        assert_eq!(plan.missing[0].yours, None);
    }

    #[test]
    fn wanted_mods_are_the_servers_the_client_lacks_or_runs_otherwise() {
        assert_eq!(
            wanted("a@1.0.0,b@0.2.0,c@1.0.0", "a@1.0.0,b@0.1.0,d@3.0.0"),
            [
                ("b".to_owned(), "0.2.0".to_owned()),
                ("c".to_owned(), "1.0.0".to_owned()),
            ]
        );
        assert!(wanted("a@1", "a@1").is_empty());
        assert!(wanted("", "a@1").is_empty());
    }

    #[test]
    fn differences_name_both_versions() {
        let names = Names::from([("d".to_owned(), "Dogs".to_owned())]);
        let lines: Vec<(bool, String)> =
            differences("a@1.0.0,b@0.2.0,c@1.0.0", "a@1.0.0,b@0.1.0,d@3.0.0", &names)
                .into_iter()
                .map(|(_, on_server, line)| (on_server, line))
                .collect();
        assert_eq!(
            lines,
            [
                (true, "b: server 0.2.0, yours 0.1.0".to_owned()),
                (true, "c: server 1.0.0, not installed".to_owned()),
                (false, "Dogs: yours 3.0.0, not on the server".to_owned()),
            ]
        );
        assert!(differences("", "", &names).is_empty());
    }
}
