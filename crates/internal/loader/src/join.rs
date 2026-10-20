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

/// The note on the server's mods, of `wanted`, that the player turned off in
/// the mod list; `None` when there are none.
#[must_use]
pub fn off_note(wanted: &[(String, String)], disabled: &[String]) -> Option<String> {
    let off: Vec<&str> = wanted
        .iter()
        .map(|(id, _)| id.as_str())
        .filter(|id| disabled.iter().any(|d| d == id))
        .collect();
    (!off.is_empty()).then(|| {
        format!(
            "You turned off {} in {}. Turn {} on there and restart the game to join.",
            off.join(", "),
            crate::modlist::LIST_PATH,
            if off.len() == 1 { "it" } else { "them" }
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

/// The name a server advertises for mod `id`; empty when it advertises none.
#[must_use]
pub fn name(reply: &[u8], id: &str) -> String {
    rule(reply, &format!("{}.{id}", crate::advertise::RULE_NAME)).unwrap_or_default()
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
    use super::super::chains::Next;
    use std::collections::BTreeMap;
    use std::sync::OnceLock;

    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW,
    };

    pub(crate) type ServerChange =
        unsafe extern "system" fn(provider: *mut u8, request: *const u8) -> u64;

    // SAFETY: the slot joins the chain of the server change, typed
    // `ServerChange`.
    /// The rest of the chain after the loader's link.
    pub(crate) static NEXT: Next<ServerChange> = unsafe { Next::new() };
    /// The request holds the address in its first 0x40 bytes.
    const ADDRESS_BYTES: usize = 0x40;

    static OURS: OnceLock<(Vec<(String, String)>, super::Names)> = OnceLock::new();

    /// Sets the client's own join mods, `(id, version)`, and the names of
    /// all its mods.
    pub(crate) fn set_ours(mods: Vec<(String, String)>, names: super::Names) {
        let _first = OURS.set((mods, names));
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
        let (Some(address), Some((mods, own_names))) = (super::address(request), OURS.get()) else {
            return true;
        };
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
        if plan.missing.is_empty() {
            crate::host::apply_join_values(&values);
        }
        for id in &plan.turn_off {
            crate::host::turn_off_mod(
                id,
                "the server does not run it; restart the game to use it again",
            );
        }
        let Some((text, blocking)) = plan.message(&names) else {
            return true;
        };
        if blocking {
            let disabled = crate::modlist::disabled();
            let wanted = server
                .as_ref()
                .map(|(_, server_mods)| super::wanted(server_mods, &ours_mods))
                .unwrap_or_default();
            match super::off_note(&wanted, disabled) {
                Some(note) => show(&format!("{text}\n\n{note}")),
                None => show(&text),
            }
            // Told after the note closes, so an updater's offer follows it.
            // Mods the player turned off are theirs to turn on, not offered.
            if server.is_some() {
                let missing: Vec<crate::host::Wanted> = wanted
                    .into_iter()
                    .filter(|(id, _)| !disabled.contains(id))
                    .map(|(id, version)| crate::host::Wanted {
                        name: super::name(&reply, &id),
                        source: super::source(&reply, &id),
                        keys: super::keys(&reply, &id),
                        id,
                        version,
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
    fn mods_turned_off_by_the_player_are_named() {
        let wanted = [
            ("safes".to_owned(), "0.3.0".to_owned()),
            ("count".to_owned(), "0.2.0".to_owned()),
        ];
        assert_eq!(off_note(&wanted, &[]), None);
        assert_eq!(off_note(&wanted, &["other".to_owned()]), None);
        assert_eq!(
            off_note(&wanted, &["safes".to_owned()]).unwrap(),
            "You turned off safes in mods/enshwick/mods.toml. Turn it on there and restart the game to join."
        );
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
