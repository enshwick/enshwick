//! The client's join check: before it joins a server, it compares the server's
//! mod set with its own. Mods the server does not run are turned off on the
//! client, with a note; when the server runs mods the client lacks, or in
//! another version, the client says which and does not join. A client that
//! joins runs its join mods with the server's join values over its own
//! settings files, and with its files' values again on joining a server
//! without them; the files stay as they are.
//!
//! A client kept out tells its mods which of the server's mods it lacks, with
//! the versions and update sources the server advertises
//! ([`Event::JOIN_MISSING`](enshwick_api::abi::Event::JOIN_MISSING)), so an
//! updater can offer them.
//!
//! The game asks to change servers with a request that starts with the server's
//! query `ip:port`. The hook lets the join go ahead and asks the server, over
//! the Steam query protocol, for its `enshwick` rule.

use std::collections::BTreeMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

use enshwick_api::mods::Updates;

/// The check holds the join, so a server that does not answer is not waited on long.
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);

/// What the client does when it joins.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// The client's mods the server does not run: turned off, with a note.
    pub turn_off: Vec<String>,
    /// The server's mods the client lacks or runs in another version: the
    /// client does not join.
    pub missing: Vec<String>,
    /// The mods match but the loader versions differ.
    pub other_loader: bool,
}

impl Plan {
    /// The note for the player, if any; `blocking` when the client stays out.
    #[must_use]
    pub fn message(&self) -> Option<(String, bool)> {
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
            return Some((
                format!(
                    "This server does not run these Enshwick mods, so they are off until you restart the game:\n\n{}",
                    self.turn_off.join("\n")
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
/// mod list), or no Enshwick at all.
#[must_use]
pub fn plan(ours: &str, ours_mods: &str, server: Option<(&str, &str)>) -> Plan {
    let ids = |list: &str| -> Vec<String> {
        list.split(',')
            .filter_map(|entry| entry.split_once('@').map(|(id, _)| id.to_owned()))
            .collect()
    };
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
            let missing: Vec<String> = differences(mods, ours_mods)
                .into_iter()
                .filter(|line| {
                    !turn_off
                        .iter()
                        .any(|id| line.starts_with(&format!("{id}: yours")))
                })
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

/// What to tell the player, or `None` when the mod sets match.
#[must_use]
pub fn verdict(ours: &str, ours_mods: &str, server: Option<(&str, &str)>) -> Option<String> {
    match server {
        Some((fingerprint, _)) if fingerprint == ours => None,
        Some((_, mods)) => {
            let lines = differences(mods, ours_mods);
            Some(if lines.is_empty() {
                "This server runs Enshwick with the same mods but another loader version, or more mods than it can list. Install the server's release, or things may not work.".to_owned()
            } else {
                format!(
                    "This server runs different Enshwick mods:\n\n{}\n\nInstall the server's versions, or things may not work.",
                    lines.join("\n")
                )
            })
        }
        None if ours_mods.is_empty() => None,
        None => Some(format!(
            "This server runs no Enshwick mods, and yours ({ours_mods}) change the game for both sides. Things may not work as expected."
        )),
    }
}

/// One line per mod whose version differs between two `id@version,...` lists,
/// naming both versions.
#[must_use]
pub fn differences(server: &str, ours: &str) -> Vec<String> {
    let parse = |list: &str| -> BTreeMap<String, String> {
        list.split(',')
            .filter_map(|entry| entry.split_once('@'))
            .map(|(id, version)| (id.to_owned(), version.to_owned()))
            .collect()
    };
    let (server, ours) = (parse(server), parse(ours));
    let mut ids: Vec<&String> = server.keys().chain(ours.keys()).collect();
    ids.sort();
    ids.dedup();
    ids.into_iter()
        .filter_map(|id| match (server.get(id), ours.get(id)) {
            (Some(theirs), Some(mine)) if theirs != mine => {
                Some(format!("{id}: server {theirs}, yours {mine}"))
            }
            (Some(theirs), None) => Some(format!("{id}: server {theirs}, not installed")),
            (None, Some(mine)) => Some(format!("{id}: yours {mine}, not on the server")),
            _ => None,
        })
        .collect()
}

/// The server's mods, `(id, version)`, that the client lacks or runs in
/// another version, from two `id@version,...` lists.
#[must_use]
pub fn wanted(server: &str, ours: &str) -> Vec<(String, String)> {
    let ours: BTreeMap<&str, &str> = ours
        .split(',')
        .filter_map(|entry| entry.split_once('@'))
        .collect();
    server
        .split(',')
        .filter_map(|entry| entry.split_once('@'))
        .filter(|(id, version)| ours.get(id) != Some(version))
        .map(|(id, version)| (id.to_owned(), version.to_owned()))
        .collect()
}

/// The update source a server advertises for mod `id`; `None` when it
/// advertises none or one this loader cannot read.
#[must_use]
pub fn source(reply: &[u8], id: &str) -> Option<Updates> {
    let line = rule(reply, &format!("{}.{id}", crate::advertise::RULE_SOURCE))?;
    Updates::parse(&line)
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

/// Asks `server` for its rules over the Steam query protocol.
///
/// # Errors
///
/// When the server does not answer in time.
pub fn query_rules(server: SocketAddr) -> std::io::Result<Vec<u8>> {
    let socket = UdpSocket::bind(if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })?;
    socket.set_read_timeout(Some(QUERY_TIMEOUT))?;
    socket.connect(server)?;
    let mut request = vec![0xff, 0xff, 0xff, 0xff, b'V', 0xff, 0xff, 0xff, 0xff];
    let mut reply = vec![0u8; 4096];
    for _ in 0..3 {
        socket.send(&request)?;
        let len = socket.recv(&mut reply)?;
        // A challenge comes back first; send it with the next request.
        if len >= 9 && reply[4] == b'A' {
            request.truncate(5);
            request.extend_from_slice(&reply[5..9]);
            continue;
        }
        reply.truncate(len);
        return Ok(reply);
    }
    Err(std::io::Error::other("no rules after three challenges"))
}

#[cfg(windows)]
pub(crate) use runtime::{NEXT, detour};

#[cfg(windows)]
mod runtime {
    use std::collections::BTreeMap;
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW,
    };

    type ServerChange = unsafe extern "system" fn(provider: *mut u8, request: *const u8) -> u64;

    /// The rest of the chain after the loader's link.
    pub(crate) static NEXT: AtomicUsize = AtomicUsize::new(0);
    /// The request holds the address in its first 0x40 bytes.
    const ADDRESS_BYTES: usize = 0x40;

    static OURS: OnceLock<Vec<(String, String)>> = OnceLock::new();

    /// Sets the client's own join mods, `(id, version)`.
    pub(crate) fn set_ours(mods: Vec<(String, String)>) {
        let _first = OURS.set(mods);
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
        let next = NEXT.load(Ordering::Acquire);
        // SAFETY: the loader points the slot at the rest of the chain, which has
        // this signature, before the link can run.
        let next: ServerChange = unsafe { core::mem::transmute(next) };
        // SAFETY: forwarded unchanged.
        unsafe { next(provider, request) }
    }

    /// Compares mod sets, turns off what the server lacks, takes the server's
    /// join values and tells the player; false when the client must not join.
    fn check(request: &[u8]) -> bool {
        let (Some(address), Some(mods)) = (super::address(request), OURS.get()) else {
            return true;
        };
        let reply = super::query_rules(address).ok();
        let Some(reply) = reply else {
            // A server that does not answer the query: nothing to compare.
            crate::log!(INFO, "joining {address}: no rules answer");
            crate::host::apply_join_values(&BTreeMap::new());
            return true;
        };
        let server = super::rule(&reply, crate::advertise::RULE)
            .map(|fingerprint| (fingerprint, super::mod_rules_value(&reply)));
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
        let plan = super::plan(
            &ours,
            &ours_mods,
            server.as_ref().map(|(f, m)| (f.as_str(), m.as_str())),
        );
        if plan.missing.is_empty() {
            crate::host::apply_join_values(&values);
        }
        for id in &plan.turn_off {
            crate::host::turn_off_mod(
                id,
                "the server does not run it; restart the game to use it again",
            );
        }
        let Some((text, blocking)) = plan.message() else {
            return true;
        };
        if blocking {
            show(&text);
            // Told after the note closes, so an updater's offer follows it.
            if let Some((_, server_mods)) = &server {
                let missing: Vec<(String, String, Option<super::Updates>)> =
                    super::wanted(server_mods, &ours_mods)
                        .into_iter()
                        .map(|(id, version)| {
                            let source = super::source(&reply, &id);
                            (id, version, source)
                        })
                        .collect();
                crate::host::report_join_missing(&missing);
            }
        } else {
            // The join goes on; the note must not hold it.
            let spawned = std::thread::Builder::new()
                .name("enshwick-join-note".into())
                .spawn(move || show(&text));
            drop(spawned);
        }
        !blocking
    }

    fn show(text: &str) {
        let text: Vec<u16> = text.encode_utf16().chain([0]).collect();
        let title: Vec<u16> = "Enshwick".encode_utf16().chain([0]).collect();
        // SAFETY: NUL-terminated strings and no owner window.
        unsafe {
            MessageBoxW(
                core::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONWARNING | MB_TOPMOST | MB_SETFOREGROUND,
            )
        };
    }
}

#[cfg(windows)]
pub(crate) use runtime::set_ours;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_sets_say_nothing_and_others_explain() {
        assert_eq!(verdict("ab", "a@1", Some(("ab", "a@1"))), None);
        assert!(
            verdict("ab", "a@1", Some(("cd", "a@2")))
                .unwrap()
                .contains("a: server 2, yours 1")
        );
        assert!(
            verdict("ab", "a@1", Some(("cd", "a@1")))
                .unwrap()
                .contains("another loader version")
        );
        assert!(
            verdict("ab", "a@1", None)
                .unwrap()
                .contains("no Enshwick mods")
        );
        assert_eq!(verdict("ab", "", None), None);
    }

    #[test]
    fn mods_the_server_lacks_are_turned_off_and_only_the_rest_warn() {
        assert_eq!(
            plan("ab", "a@1,b@1", Some(("ab", "a@1,b@1"))),
            Plan::default()
        );
        let none = plan("ab", "a@1,b@1", None);
        assert_eq!(none.turn_off, ["a", "b"]);
        assert_eq!(none.message().map(|(_, blocking)| blocking), Some(false));
        let extra = plan("ab", "a@1,b@1", Some(("cd", "a@1")));
        assert_eq!(extra.turn_off, ["b"]);
        assert!(extra.missing.is_empty());
        let missing = plan("ab", "a@1", Some(("cd", "a@2,c@1")));
        assert!(missing.turn_off.is_empty());
        let (text, blocking) = missing.message().unwrap();
        assert!(blocking);
        assert!(
            text.contains("a: server 2, yours 1") && text.contains("c: server 1, not installed")
        );
        assert!(plan("ab", "a@1", Some(("cd", "a@1"))).other_loader);
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
    fn differences_name_both_versions() {
        assert_eq!(
            differences("a@1.0.0,b@0.2.0,c@1.0.0", "a@1.0.0,b@0.1.0,d@3.0.0"),
            [
                "b: server 0.2.0, yours 0.1.0",
                "c: server 1.0.0, not installed",
                "d: yours 3.0.0, not on the server",
            ]
        );
        assert!(differences("", "").is_empty());
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
