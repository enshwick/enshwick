//! The client's join check: before it joins a server, it compares the server's
//! mod set with its own. Mods the server does not run are turned off on the
//! client, with a note; when the server runs mods the client lacks, or in
//! another version, the client says which and does not join. A client that
//! joins runs its join mods with the server's join values over its own
//! settings files, and with its files' values again on joining a server
//! without them; the files stay as they are.
//!
//! A client kept out tells its mods which of the server's mods it lacks, with
//! the versions, update sources and author keys the server advertises
//! ([`Event::JOIN_MISSING`](enshwick_loader_api::abi::Event::JOIN_MISSING)), so an
//! updater can offer them.
//!
//! The game asks to change servers with a request that starts with the server's
//! query `ip:port`. The hook lets the join go ahead and asks the server, over
//! the Steam query protocol, for its `enshwick` rule.

use std::collections::BTreeMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use enshwick_loader_api::mods::Updates;

/// The check holds the join, so a server that does not answer is not waited on long.
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);

/// The names players read, by mod id: the client's own and those the server
/// advertises.
pub type Names = BTreeMap<String, String>;

/// The longest mod name a server may advertise, in characters.
const MAX_NAME: usize = 64;
/// The longest version a server may advertise, in bytes.
const MAX_VERSION: usize = 64;

/// The `(id, version)` entries of an `id@version,...` list, without an id
/// that is no mod id or a version that is no version: a server writes the
/// list, and its ids and versions reach the player's notes and the updater.
fn entries(list: &str) -> impl Iterator<Item = (&str, &str)> {
    list.split(',')
        .filter_map(|entry| entry.split_once('@'))
        .filter(|&(id, version)| {
            enshwick_loader_api::abi::is_valid_id(id)
                && !version.is_empty()
                && version.len() <= MAX_VERSION
                && version
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'-'))
        })
}

/// `Name (id)` of mod `id`, or the id when `names` has none.
fn label(names: &Names, id: &str) -> String {
    enshwick_loader_api::mods::label(names.get(id).map_or("", String::as_str), id)
}

/// What the client does when it joins.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// The client's mods the server does not run: turned off, with a note.
    pub turn_off: Vec<String>,
    /// The server's mods the client lacks or runs in another version, as
    /// lines for the note: the client does not join.
    pub missing: Vec<String>,
    /// The mods match but the loader versions differ.
    pub other_loader: bool,
}

impl Plan {
    /// Whether the client joins: the server runs no mod it lacks.
    #[must_use]
    pub fn joins(&self) -> bool {
        self.missing.is_empty()
    }

    /// The client's mods to turn off now: none when it stays out, since
    /// it never reaches the server that lacks them.
    #[must_use]
    pub fn turned_off(&self) -> &[String] {
        if self.joins() { &self.turn_off } else { &[] }
    }

    /// The note for the player, if any, naming mods by `names`; `blocking`
    /// when the client stays out.
    #[must_use]
    pub fn message(&self, names: &Names) -> Option<(String, bool)> {
        if !self.missing.is_empty() {
            return Some((
                format!(
                    "This server runs Enshwick mods you do not have in its version:\n\n{}\n\nInstall the server's versions to join.",
                    self.missing.join("\n")
                ),
                true,
            ));
        }
        if !self.turn_off.is_empty() {
            let lines: Vec<String> = self.turn_off.iter().map(|id| label(names, id)).collect();
            return Some((
                format!(
                    "This server does not run these Enshwick mods, so they are off until you restart the game:\n\n{}",
                    lines.join("\n")
                ),
                false,
            ));
        }
        self.other_loader.then(|| {
            (
                "This server runs Enshwick with the same mods but another loader version. Install the server's release, or things may not work.".to_owned(),
                false,
            )
        })
    }
}

/// The plan for joining a server that advertises `server` (fingerprint and
/// mod list), or no Enshwick at all; notes name mods by `names`.
#[must_use]
pub fn plan(ours: &str, ours_mods: &str, server: Option<(&str, &str)>, names: &Names) -> Plan {
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
                .filter(|id| !theirs.contains(id))
                .collect();
            let missing: Vec<String> = differences(mods, ours_mods, names)
                .into_iter()
                .filter(|(_, on_server, _)| *on_server)
                .map(|(_, _, line)| line)
                .collect();
            let other_loader = missing.is_empty() && turn_off.is_empty();
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
#[must_use]
pub fn wanted(server: &str, ours: &str) -> Vec<(String, String)> {
    let ours: BTreeMap<&str, &str> = entries(ours).collect();
    entries(server)
        .filter(|(id, version)| ours.get(id) != Some(version))
        .map(|(id, version)| (id.to_owned(), version.to_owned()))
        .collect()
}

/// The note on the server's mods, of `wanted`, that the player turned off in
/// the mod list (`disabled`) or that this game turned off since it started
/// (`session_off`); `None` when there are none.
#[must_use]
pub fn off_note(
    wanted: &[(String, String)],
    disabled: &[String],
    session_off: &[String],
) -> Option<String> {
    let among = |list: &[String]| -> Vec<&str> {
        wanted
            .iter()
            .map(|(id, _)| id.as_str())
            .filter(|id| list.iter().any(|d| d == id))
            .collect()
    };
    let them = |ids: &[&str]| if ids.len() == 1 { "it" } else { "them" };
    let (player, session) = (among(disabled), among(session_off));
    let mut lines = Vec::new();
    if !player.is_empty() {
        lines.push(format!(
            "You turned off {} in {}. Turn {} on there and restart the game to join.",
            player.join(", "),
            crate::modlist::LIST_PATH,
            them(&player)
        ));
    }
    if !session.is_empty() {
        lines.push(format!(
            "{} turned off while this game ran (see the log). Restart the game to join with {}.",
            session.join(", "),
            them(&session)
        ));
    }
    (!lines.is_empty()).then(|| {
        lines.join(
            "

",
        )
    })
}

/// The update source a server advertises for mod `id`; `None` when it
/// advertises none or one this loader cannot read.
#[must_use]
pub fn source(reply: &[u8], id: &str) -> Option<Updates> {
    let line = rule(reply, &format!("{}.{id}", crate::advertise::RULE_SOURCE))?;
    Updates::parse(&line)
}

/// The name a server advertises for mod `id`, on one line and at most
/// [`MAX_NAME`] characters; empty when it advertises none.
#[must_use]
pub fn name(reply: &[u8], id: &str) -> String {
    rule(reply, &format!("{}.{id}", crate::advertise::RULE_NAME))
        .map(|name| {
            name.chars()
                .filter(|c| !c.is_control())
                .take(MAX_NAME)
                .collect::<String>()
                .trim()
                .to_owned()
        })
        .unwrap_or_default()
}

/// The author keys a server advertises for mod `id`; empty when it
/// advertises none or a line this loader cannot read.
#[must_use]
pub fn keys(reply: &[u8], id: &str) -> Vec<String> {
    rule(reply, &format!("{}.{id}", crate::advertise::RULE_KEY))
        .map(|line| enshwick_loader_api::mods::parse_keys(&line))
        .unwrap_or_default()
}

/// The whole mod list a server advertises across its mod rules.
#[must_use]
pub fn mod_rules_value(reply: &[u8]) -> String {
    (0..crate::advertise::MOD_RULES)
        .map_while(|i| rule(reply, &crate::advertise::rule_name(i)))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

/// The join values a server advertises for mod `id` across its rules, `None`
/// when it advertises none.
#[must_use]
pub fn sync_value(reply: &[u8], id: &str) -> Option<String> {
    let base = format!("{}.{id}", crate::advertise::RULE_SYNC);
    let value: String = (0..crate::advertise::SYNC_RULES)
        .map_while(|i| rule(reply, &crate::advertise::part_name(&base, i)))
        .collect();
    (!value.is_empty()).then_some(value)
}

/// The join values a server advertises for each of the mods `ids`.
#[must_use]
pub fn server_values<'a>(
    reply: &[u8],
    ids: impl IntoIterator<Item = &'a str>,
) -> BTreeMap<String, String> {
    ids.into_iter()
        .filter_map(|id| Some((id.to_owned(), sync_value(reply, id)?)))
        .collect()
}

/// The mods whose join values change on joining a server with `incoming`,
/// from each mod's id (empty for a mod that does not join) and the server
/// values it runs with now: the mod's index and the values it runs with
/// next, `None` for its settings file's.
#[must_use]
pub fn changes<'a>(
    current: impl IntoIterator<Item = (&'a str, Option<&'a str>)>,
    incoming: &BTreeMap<String, String>,
) -> Vec<(usize, Option<String>)> {
    current
        .into_iter()
        .enumerate()
        .filter(|(_, (id, _))| !id.is_empty())
        .filter_map(|(index, (id, held))| {
            let next = incoming.get(id).map(String::as_str);
            (next != held).then(|| (index, next.map(str::to_owned)))
        })
        .collect()
}

/// The `ip:port` at the start of a server change request.
#[must_use]
pub fn address(request: &[u8]) -> Option<SocketAddr> {
    let end = request.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&request[..end]).ok()?.parse().ok()
}

/// The value of `key` in an `A2S_RULES` reply.
#[must_use]
pub fn rule(reply: &[u8], key: &str) -> Option<String> {
    let body = reply.strip_prefix(&[0xff, 0xff, 0xff, 0xff, b'E'])?;
    let mut fields = body.get(2..)?.split(|&b| b == 0);
    while let (Some(name), Some(value)) = (fields.next(), fields.next()) {
        if name == key.as_bytes() {
            return String::from_utf8(value.to_vec()).ok();
        }
    }
    None
}

/// The most packets a split reply may take.
const MAX_PACKETS: u8 = 16;
/// The header of one packet of a split reply.
const SPLIT: [u8; 4] = [0xfe, 0xff, 0xff, 0xff];
/// The header of a whole reply.
const WHOLE: [u8; 4] = [0xff, 0xff, 0xff, 0xff];

/// The packets of a reply the server split, as the Steam query protocol
/// splits a reply larger than one datagram: each packet carries the reply's
/// id, the count of packets, its number and its size before its part.
#[derive(Debug, Default)]
struct Split {
    id: u32,
    parts: Vec<Option<Vec<u8>>>,
}

impl Split {
    /// Takes one packet; the whole reply once every part is in, or an error
    /// for a packet that breaks the protocol or a compressed reply.
    fn add(&mut self, packet: &[u8]) -> Result<Option<Vec<u8>>, &'static str> {
        let body = packet
            .strip_prefix(&SPLIT)
            .ok_or("a whole reply among parts")?;
        let (&[a, b, c, d, total, number, _, _], part) = body
            .split_first_chunk::<8>()
            .ok_or("a packet shorter than its header")?;
        let id = u32::from_le_bytes([a, b, c, d]);
        if id & 0x8000_0000 != 0 {
            return Err("a compressed reply");
        }
        if total == 0 || total > MAX_PACKETS || number >= total {
            return Err("a packet count out of range");
        }
        if self.parts.is_empty() {
            self.id = id;
            self.parts = vec![None; usize::from(total)];
        } else if id != self.id || self.parts.len() != usize::from(total) {
            // A packet of an older reply: skip it.
            return Ok(None);
        }
        if let Some(slot) = self.parts.get_mut(usize::from(number)) {
            *slot = Some(part.to_vec());
        }
        if self.parts.iter().any(Option::is_none) {
            return Ok(None);
        }
        Ok(Some(
            self.parts.iter().flatten().flatten().copied().collect(),
        ))
    }
}

/// Asks `server` for its rules over the Steam query protocol.
///
/// # Errors
///
/// When the server does not answer in time, or not with rules.
pub fn query_rules(server: SocketAddr) -> std::io::Result<Vec<u8>> {
    let socket = UdpSocket::bind(if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })?;
    socket.set_read_timeout(Some(QUERY_TIMEOUT))?;
    socket.connect(server)?;
    let mut request = vec![0xff, 0xff, 0xff, 0xff, b'V', 0xff, 0xff, 0xff, 0xff];
    // A reply larger than the buffer fails to read on Windows, which would
    // let the join through unchecked.
    let mut reply = vec![0u8; 65_536];
    for _ in 0..3 {
        socket.send(&request)?;
        let len = socket.recv(&mut reply)?;
        // A challenge comes back first; send it with the next request.
        if len >= 9 && reply[4] == b'A' {
            request.truncate(5);
            request.extend_from_slice(&reply[5..9]);
            continue;
        }
        let mut whole = reply[..len].to_vec();
        if whole.starts_with(&SPLIT) {
            let mut split = Split::default();
            let mut next = split.add(&whole);
            for _ in 0..MAX_PACKETS * 2 {
                match next {
                    Ok(Some(_)) | Err(_) => break,
                    Ok(None) => {
                        let len = socket.recv(&mut reply)?;
                        next = split.add(&reply[..len]);
                    }
                }
            }
            whole = match next {
                Ok(Some(whole)) => whole,
                Ok(None) => return Err(std::io::Error::other("a split reply never completed")),
                Err(why) => return Err(std::io::Error::other(why)),
            };
        }
        if !whole.starts_with(&WHOLE) || whole.get(4) != Some(&b'E') {
            return Err(std::io::Error::other("a reply that holds no rules"));
        }
        return Ok(whole);
    }
    Err(std::io::Error::other("no rules after three challenges"))
}

#[cfg(windows)]
pub(crate) use runtime::{NEXT, detour};

#[cfg(windows)]
mod runtime {
    use super::super::chains::Next;
    use std::collections::BTreeMap;
    use std::sync::OnceLock;

    use enshwick_loader_api::abi::UiDialog;

    pub(crate) type ServerChange =
        unsafe extern "system" fn(provider: *mut u8, request: *const u8) -> u64;

    // SAFETY: the slot joins the chain of the server change, typed
    // `ServerChange`.
    /// The rest of the chain after the loader's link.
    pub(crate) static NEXT: Next<ServerChange> = unsafe { Next::new() };
    /// The request holds the address in its first 0x40 bytes.
    const ADDRESS_BYTES: usize = 0x40;

    static NAMES: OnceLock<super::Names> = OnceLock::new();

    /// Sets the names of all the client's mods.
    pub(crate) fn set_names(names: super::Names) {
        let _first = NAMES.set(names);
    }

    /// The loader's link on `server_change`.
    pub(crate) unsafe extern "system" fn detour(provider: *mut u8, request: *const u8) -> u64 {
        if !request.is_null() {
            // SAFETY: the request holds at least the address bytes.
            let bytes = unsafe { std::slice::from_raw_parts(request, ADDRESS_BYTES) };
            let allowed = std::panic::catch_unwind(|| check(bytes)).unwrap_or(true);
            if !allowed {
                crate::log!(WARN, "join refused: the server runs mods this client lacks");
                return 0;
            }
        }
        // SAFETY: forwarded unchanged.
        unsafe { NEXT.get()(provider, request) }
    }

    /// Compares mod sets, turns off what the server lacks, takes the server's
    /// join values and tells the player; false when the client must not join.
    fn check(request: &[u8]) -> bool {
        let (Some(address), Some(own_names)) = (super::address(request), NAMES.get()) else {
            return true;
        };
        // Read now: a mod an earlier server or a failed hook turned off no
        // longer counts as the client's.
        let mods = &crate::host::join_mods();
        let reply = super::query_rules(address).ok();
        let Some(reply) = reply else {
            // A server that does not answer the query: nothing to compare.
            crate::log!(INFO, "joining {address}: no rules answer");
            crate::net::joining(false);
            crate::host::apply_join_values(&BTreeMap::new());
            return true;
        };
        let server = super::rule(&reply, crate::advertise::RULE)
            .map(|fingerprint| (fingerprint, super::mod_rules_value(&reply)));
        crate::net::joining(server.is_some());
        // The client runs with the server's join values, so its own
        // fingerprint counts them.
        let values = if server.is_some() {
            super::server_values(&reply, mods.iter().map(|(id, _)| id.as_str()))
        } else {
            BTreeMap::new()
        };
        let ours = crate::advertise::fingerprint(env!("CARGO_PKG_VERSION"), mods, &values);
        let ours_mods = crate::advertise::mod_list(mods);
        crate::log!(INFO, "joining {address}: server {server:?}, ours {ours}");
        let mut names = own_names.clone();
        if let Some((_, server_mods)) = &server {
            for (id, _) in super::wanted(server_mods, "") {
                let name = super::name(&reply, &id);
                if !name.is_empty() {
                    names.entry(id).or_insert(name);
                }
            }
        }
        let plan = super::plan(
            &ours,
            &ours_mods,
            server.as_ref().map(|(f, m)| (f.as_str(), m.as_str())),
            &names,
        );
        if plan.joins() {
            crate::host::apply_join_values(&values);
        }
        for id in plan.turned_off() {
            crate::host::turn_off_mod(
                id,
                "the server does not run it; restart the game to use it again",
            );
            crate::reload::hold(id);
        }
        let Some((text, blocking)) = plan.message(&names) else {
            return true;
        };
        if !blocking {
            // The join goes on, and the note shows over the game.
            crate::ui::show("Enshwick", &text, "", 1 << UiDialog::OK, |_| {});
            return true;
        }
        let disabled = crate::modlist::disabled();
        let session_off = crate::host::join_mods_off();
        let wanted = server
            .as_ref()
            .map(|(_, server_mods)| super::wanted(server_mods, &ours_mods))
            .unwrap_or_default();
        let text = match super::off_note(&wanted, disabled, &session_off) {
            Some(note) => format!("{text}\n\n{note}"),
            None => text,
        };
        // Mods the player turned off are theirs to turn on, not offered.
        let missing: Vec<crate::host::Wanted> = if server.is_some() {
            wanted
                .into_iter()
                .filter(|(id, _)| !disabled.contains(id) && !session_off.contains(id))
                .map(|(id, version)| crate::host::Wanted {
                    name: super::name(&reply, &id),
                    source: super::source(&reply, &id),
                    keys: super::keys(&reply, &id),
                    id,
                    version,
                })
                .collect()
        } else {
            Vec::new()
        };
        // The join is refused at once, so the game keeps drawing; the
        // updater's offer follows once the player closes the note.
        crate::ui::show("Enshwick", &text, "", 1 << UiDialog::OK, move |_| {
            if !missing.is_empty() {
                crate::host::report_join_missing(&missing);
            }
        });
        false
    }
}

#[cfg(windows)]
pub(crate) use runtime::set_names;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mods_the_server_lacks_are_turned_off_and_only_the_rest_warn() {
        let names = Names::from([("c".to_owned(), "Cats".to_owned())]);
        assert_eq!(
            plan("ab", "a@1,b@1", Some(("ab", "a@1,b@1")), &names),
            Plan::default()
        );
        let none = plan("ab", "a@1,b@1", None, &names);
        assert_eq!(none.turn_off, ["a", "b"]);
        assert_eq!(
            none.message(&names).map(|(_, blocking)| blocking),
            Some(false)
        );
        let extra = plan("ab", "a@1,b@1", Some(("cd", "a@1")), &names);
        assert_eq!(extra.turn_off, ["b"]);
        assert!(extra.missing.is_empty());
        let missing = plan("ab", "a@1", Some(("cd", "a@2,c@1")), &names);
        assert!(missing.turn_off.is_empty());
        let (text, blocking) = missing.message(&names).unwrap();
        assert!(blocking);
        assert!(
            text.contains("a: server 2, yours 1")
                && text.contains("Cats (c): server 1, not installed"),
            "{text}"
        );
        assert!(plan("ab", "a@1", Some(("cd", "a@1")), &names).other_loader);
    }

    #[test]
    fn a_client_kept_out_turns_nothing_off() {
        let names = Names::new();
        let both = plan("ab", "a@1,b@1", Some(("cd", "a@2")), &names);
        assert_eq!(both.turn_off, ["b"]);
        assert!(!both.joins());
        assert!(both.turned_off().is_empty());
        let extra = plan("ab", "a@1,b@1", Some(("cd", "a@1")), &names);
        assert!(extra.joins());
        assert_eq!(extra.turned_off(), ["b"]);
    }

    #[test]
    fn mods_turned_off_by_the_player_are_named() {
        let wanted = [
            ("safes".to_owned(), "0.3.0".to_owned()),
            ("count".to_owned(), "0.2.0".to_owned()),
        ];
        assert_eq!(off_note(&wanted, &[], &[]), None);
        assert_eq!(off_note(&wanted, &["other".to_owned()], &[]), None);
        assert_eq!(
            off_note(&wanted, &["safes".to_owned()], &[]).unwrap(),
            "You turned off safes in mods/enshwick/mods.toml. Turn it on there and restart the game to join."
        );
    }

    #[test]
    fn mods_turned_off_since_the_game_started_are_named() {
        // Server A lacked count, so it turned off; server B runs it.
        let wanted = [("count".to_owned(), "0.2.0".to_owned())];
        assert_eq!(
            off_note(&wanted, &[], &["count".to_owned(), "other".to_owned()]).unwrap(),
            "count turned off while this game ran (see the log). Restart the game to join with it."
        );
        let both = off_note(&wanted, &["count".to_owned()], &["count".to_owned()]).unwrap();
        assert!(
            both.contains("You turned off count")
                && both.contains(
                    "

count turned off"
                )
        );
        // The client's set without the mod that is off: the join is refused.
        let names = Names::new();
        let plan = plan("x", "", Some(("y", "count@0.2.0")), &names);
        assert!(!plan.joins());
    }

    #[test]
    fn a_split_reply_is_put_back_together() {
        let whole: Vec<u8> = [0xff, 0xff, 0xff, 0xff, b'E', 1, 0]
            .into_iter()
            .chain(*b"enshwick abcd ")
            .collect();
        let packet = |id: u32, total: u8, number: u8, part: &[u8]| -> Vec<u8> {
            let mut packet = SPLIT.to_vec();
            packet.extend_from_slice(&id.to_le_bytes());
            packet.extend_from_slice(&[total, number, 0xe0, 0x04]);
            packet.extend_from_slice(part);
            packet
        };
        let (first, second) = whole.split_at(9);
        let mut split = Split::default();
        assert_eq!(split.add(&packet(7, 2, 1, second)), Ok(None));
        assert_eq!(split.add(&packet(6, 2, 0, b"old")), Ok(None));
        let joined = split.add(&packet(7, 2, 0, first)).unwrap().unwrap();
        assert_eq!(joined, whole);
        assert_eq!(rule(&joined, "enshwick").as_deref(), Some("abcd"));
        let mut split = Split::default();
        assert!(split.add(&packet(0x8000_0001, 2, 0, first)).is_err());
        assert!(Split::default().add(&packet(1, 0, 0, first)).is_err());
        assert!(Split::default().add(&packet(1, 2, 2, first)).is_err());
        assert!(Split::default().add(&packet(1, 17, 0, first)).is_err());
        assert!(Split::default().add(&SPLIT).is_err());
        assert!(Split::default().add(&whole).is_err());
    }

    #[test]
    fn the_mod_list_is_read_across_rules() {
        let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 3, 0];
        for part in [
            "enshwick.mods",
            "a@1,b@1",
            "enshwick.mods.1",
            "c@1",
            "enshwick",
            "x",
        ] {
            reply.extend_from_slice(part.as_bytes());
            reply.push(0);
        }
        assert_eq!(mod_rules_value(&reply), "a@1,b@1,c@1");
    }

    fn reply(rules: &[(&str, &str)]) -> Vec<u8> {
        let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 0, 0];
        for (key, value) in rules {
            for part in [key, value] {
                reply.extend_from_slice(part.as_bytes());
                reply.push(0);
            }
        }
        reply
    }

    #[test]
    fn join_values_are_read_across_rules_per_mod() {
        let advertised = crate::advertise::sync_rules("count", &"x".repeat(200)).unwrap();
        let mut rules: Vec<(&str, &str)> = advertised
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        rules.push(("enshwick.sync.zones", "{ a = 1 }"));
        rules.push(("enshwick.sync.gone", ""));
        let reply = reply(&rules);
        assert_eq!(sync_value(&reply, "count"), Some("x".repeat(200)));
        assert_eq!(sync_value(&reply, "gone"), None);
        assert_eq!(sync_value(&reply, "missing"), None);
        assert_eq!(
            server_values(&reply, ["zones", "gone", "other"]),
            BTreeMap::from([("zones".to_owned(), "{ a = 1 }".to_owned())])
        );
    }

    #[test]
    fn only_mods_whose_join_values_change_are_told() {
        let incoming = BTreeMap::from([
            ("a".to_owned(), "{ x = 1 }".to_owned()),
            ("b".to_owned(), "{ y = 2 }".to_owned()),
        ]);
        let current = [
            ("a", Some("{ x = 1 }")),
            ("b", None),
            ("c", Some("{ z = 3 }")),
            ("", None),
            ("d", None),
        ];
        assert_eq!(
            changes(current, &incoming),
            [(1, Some("{ y = 2 }".to_owned())), (2, None)]
        );
        assert!(changes([("a", None)], &BTreeMap::new()).is_empty());
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
    fn sources_are_read_from_the_rules() {
        let reply = reply(&[
            ("enshwick.src.a", "github:themaiby/enshwick:enshwick-a"),
            ("enshwick.src.b", "nonsense"),
        ]);
        assert_eq!(
            source(&reply, "a"),
            Some(Updates::GitHub {
                repository: "themaiby/enshwick".into(),
                component: "enshwick-a".into(),
            })
        );
        assert_eq!(source(&reply, "b"), None);
        assert_eq!(source(&reply, "c"), None);
    }

    #[test]
    fn keys_are_read_from_the_rules() {
        let key = "RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF";
        let reply = reply(&[("enshwick.key.a", key), ("enshwick.key.b", "RWnot-a-key")]);
        assert_eq!(keys(&reply, "a"), [key]);
        assert!(keys(&reply, "b").is_empty());
        assert!(keys(&reply, "c").is_empty());
    }

    #[test]
    fn a_server_s_ids_versions_and_names_are_checked_before_anyone_reads_them() {
        let server =
            "ok@1.2.0,../evil@1.0.0,x\ny@1.0.0,Caps@1.0.0,pack@0.1.0+ab12,bad@1.0\nInstall,@1";
        assert_eq!(
            wanted(server, ""),
            [
                ("ok".to_owned(), "1.2.0".to_owned()),
                ("pack".to_owned(), "0.1.0+ab12".to_owned())
            ]
        );
        let missing = plan("ab", "", Some(("cd", server)), &Names::new());
        assert_eq!(missing.missing.len(), 2, "{:?}", missing.missing);
        let reply = reply(&[
            ("enshwick.name.ok", "Safes\n\nVerified by Enshwick"),
            ("enshwick.name.long", &"n".repeat(500)),
        ]);
        assert_eq!(name(&reply, "ok"), "SafesVerified by Enshwick");
        assert_eq!(name(&reply, "long").chars().count(), MAX_NAME);
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
                (false, "Dogs (d): yours 3.0.0, not on the server".to_owned()),
            ]
        );
        assert!(differences("", "", &names).is_empty());
    }

    #[test]
    fn the_address_is_read_up_to_the_first_nul() {
        let mut request = [0u8; 0x40];
        request[..15].copy_from_slice(b"127.0.0.1:15647");
        assert_eq!(address(&request), Some("127.0.0.1:15647".parse().unwrap()));
    }

    #[test]
    fn a_rule_is_found_in_a_rules_reply() {
        let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 2, 0];
        for part in ["enshwick", "989b6a71", "enshwick.mods", "a@1"] {
            reply.extend_from_slice(part.as_bytes());
            reply.push(0);
        }
        assert_eq!(rule(&reply, "enshwick").as_deref(), Some("989b6a71"));
        assert_eq!(rule(&reply, "enshwick.mods").as_deref(), Some("a@1"));
        assert_eq!(rule(&reply, "missing"), None);
    }
}
