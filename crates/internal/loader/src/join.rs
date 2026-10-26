//! The client's join check: before it joins a server, it compares the server's
//! mod set with its own. Mods the server does not run are turned off on the
//! client, with a note; when the server runs mods the client lacks, or in
//! another version, the client says which and does not join. A client that
//! joins runs its join mods with the server's join values over its own
//! settings files, and with its files' values again on joining a server
//! without them; the files stay as they are.
//!
//! A client kept out shows one dialog naming each of the server's mods it
//! lacks, the server's version and where to get it ([`refusal`]), and tells
//! its mods which they are, with the versions and homepages the server
//! advertises
//! ([`Event::JOIN_REFUSED`](enshwick_loader_api::abi::Event::JOIN_REFUSED)).
//!
//! Every join from the game's menus hands its online provider a target with
//! the server's address and query port. The hook asks the server, over the
//! Steam query protocol, for its `enshwick` rule before the game connects;
//! a refused join goes on without its host, so the game's own join step fails
//! and the game goes back to its menus, its own dialog held back
//! ([`interaction`]).

use std::collections::BTreeMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use crate::admission::Need;
use crate::texts;

pub mod interaction;
pub mod provider;
pub mod refusal;

/// The check holds the join, so a server that does not answer is not waited on long.
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);

/// The names players read, by mod id: the client's own and those the server
/// advertises.
///
/// # Examples
///
/// ```
/// use enshwick::join::Names;
///
/// let mut names = Names::new();
/// names.insert("safes".to_owned(), "Safes".to_owned());
/// assert_eq!(names["safes"], "Safes");
/// ```
pub type Names = BTreeMap<String, String>;

/// The longest mod name a server may advertise, in characters.
const MAX_NAME: usize = 64;
/// The longest version a server may advertise, in bytes.
const MAX_VERSION: usize = 64;

/// The `(id, version)` entries of an `id@version,...` list, without an id
/// that is no mod id or a version that is no version: a server writes the
/// list, and its ids and versions reach the player's notes and the mods.
pub(crate) fn entries(list: &str) -> impl Iterator<Item = (&str, &str)> {
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

/// The name players read for mod `id`, or the id when `names` has none.
fn label(names: &Names, id: &str) -> String {
    enshwick_loader_api::mods::label(names.get(id).map_or("", String::as_str), id)
}

/// What the client does when it joins.
///
/// # Examples
///
/// ```
/// use enshwick::join::plan;
///
/// let plan = plan("e5db5dca", "safes@1.0.0", Some(("989b6a71", "safes@2.0.0")), true);
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
    /// use enshwick::join::Plan;
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
    /// use enshwick::join::Plan;
    ///
    /// let plan = Plan { turn_off: vec!["safes".to_owned()], ..Plan::default() };
    /// assert_eq!(plan.turned_off(), ["safes"]);
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
    /// use enshwick::join::{Names, Plan};
    ///
    /// let plan = Plan { turn_off: vec!["safes".to_owned()], ..Plan::default() };
    /// assert!(plan.note(&Names::new()).is_some_and(|note| note.ends_with("safes")));
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

/// The plan for joining a server that advertises `server` (fingerprint and
/// mod list), or no Enshwick at all. A list that is not `complete` holds only
/// the first of the server's mods, so a mod of ours it does not name may
/// still run there and stays on.
///
/// # Examples
///
/// ```
/// use enshwick::join::plan;
///
/// let plan = plan("e5db5dca", "safes@1.0.0,camera@1.0.0", Some(("989b6a71", "safes@1.0.0")), true);
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
/// use enshwick::join::{Names, differences};
///
/// let found = differences("safes@2.0.0", "safes@1.0.0", &Names::new());
/// assert_eq!(found, [("safes".to_owned(), true, "safes: server 2.0.0, yours 1.0.0".to_owned())]);
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
/// use enshwick::join::wanted;
///
/// let wanted = wanted("safes@2.0.0,camera@1.0.0", "camera@1.0.0");
/// assert_eq!(wanted, [("safes".to_owned(), "2.0.0".to_owned())]);
/// ```
#[must_use]
pub fn wanted(server: &str, ours: &str) -> Vec<(String, String)> {
    let ours: BTreeMap<&str, &str> = entries(ours).collect();
    entries(server)
        .filter(|(id, version)| ours.get(id) != Some(version))
        .map(|(id, version)| (id.to_owned(), version.to_owned()))
        .collect()
}

/// The homepage a server advertises for mod `id`, as a link players can
/// open; `None` when it advertises none or something else.
///
/// # Examples
///
/// ```
/// # fn reply(rules: &[(&str, &str)]) -> Vec<u8> {
/// #     let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 0, 0];
/// #     for (key, value) in rules {
/// #         reply.extend_from_slice(key.as_bytes());
/// #         reply.push(0);
/// #         reply.extend_from_slice(value.as_bytes());
/// #         reply.push(0);
/// #     }
/// #     reply
/// # }
/// use enshwick::join::page;
///
/// let reply = reply(&[("enshwick.page.safes", "https://github.com/themaiby/enshwick")]);
/// assert!(page(&reply, "safes").is_some());
/// assert!(page(&reply, "camera").is_none());
/// ```
#[must_use]
pub fn page(reply: &[u8], id: &str) -> Option<String> {
    let page = rule(reply, &format!("{}.{id}", crate::advertise::RULE_PAGE))?;
    crate::admission::place(&page)
}

/// The name a server advertises for mod `id`, on one line and at most
/// [`MAX_NAME`] characters; empty when it advertises none.
///
/// # Examples
///
/// ```
/// # fn reply(rules: &[(&str, &str)]) -> Vec<u8> {
/// #     let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 0, 0];
/// #     for (key, value) in rules {
/// #         reply.extend_from_slice(key.as_bytes());
/// #         reply.push(0);
/// #         reply.extend_from_slice(value.as_bytes());
/// #         reply.push(0);
/// #     }
/// #     reply
/// # }
/// use enshwick::join::name;
///
/// let reply = reply(&[("enshwick.name.safes", "Safes")]);
/// assert_eq!(name(&reply, "safes"), "Safes");
/// assert_eq!(name(&reply, "camera"), "");
/// ```
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

/// The whole mod list a server advertises across its mod rules.
///
/// # Examples
///
/// ```
/// # fn reply(rules: &[(&str, &str)]) -> Vec<u8> {
/// #     let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 0, 0];
/// #     for (key, value) in rules {
/// #         reply.extend_from_slice(key.as_bytes());
/// #         reply.push(0);
/// #         reply.extend_from_slice(value.as_bytes());
/// #         reply.push(0);
/// #     }
/// #     reply
/// # }
/// use enshwick::join::mod_rules_value;
///
/// let reply = reply(&[("enshwick.mods", "safes@1.0.0"), ("enshwick.mods.1", "camera@1.0.0")]);
/// assert_eq!(mod_rules_value(&reply), "safes@1.0.0,camera@1.0.0");
/// ```
#[must_use]
pub fn mod_rules_value(reply: &[u8]) -> String {
    (0..crate::advertise::MOD_RULES)
        .map_while(|i| rule(reply, &crate::advertise::rule_name(i)))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(",")
}

/// Whether the mod list across the mod rules is the server's whole list: a
/// server whose list did not fit gives its number of mods in
/// [`crate::advertise::RULE_MOD_COUNT`].
///
/// # Examples
///
/// ```
/// # fn reply(rules: &[(&str, &str)]) -> Vec<u8> {
/// #     let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 0, 0];
/// #     for (key, value) in rules {
/// #         reply.extend_from_slice(key.as_bytes());
/// #         reply.push(0);
/// #         reply.extend_from_slice(value.as_bytes());
/// #         reply.push(0);
/// #     }
/// #     reply
/// # }
/// use enshwick::join::mods_complete;
///
/// let cut = reply(&[("enshwick.mods", "safes@1.0.0"), ("enshwick.mods.count", "2")]);
/// assert!(!mods_complete(&cut));
/// assert!(mods_complete(&reply(&[("enshwick.mods", "safes@1.0.0")])));
/// ```
#[must_use]
pub fn mods_complete(reply: &[u8]) -> bool {
    rule(reply, crate::advertise::RULE_MOD_COUNT)
        .and_then(|count| count.parse::<usize>().ok())
        .is_none_or(|count| entries(&mod_rules_value(reply)).count() >= count)
}

/// The join values a server advertises for mod `id` across its rules, `None`
/// when it advertises none.
///
/// # Examples
///
/// ```
/// # fn reply(rules: &[(&str, &str)]) -> Vec<u8> {
/// #     let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 0, 0];
/// #     for (key, value) in rules {
/// #         reply.extend_from_slice(key.as_bytes());
/// #         reply.push(0);
/// #         reply.extend_from_slice(value.as_bytes());
/// #         reply.push(0);
/// #     }
/// #     reply
/// # }
/// use enshwick::join::sync_value;
///
/// let reply = reply(&[("enshwick.sync.safes", "{ cost_multiplier = 2 }")]);
/// assert_eq!(sync_value(&reply, "safes").as_deref(), Some("{ cost_multiplier = 2 }"));
/// ```
#[must_use]
pub fn sync_value(reply: &[u8], id: &str) -> Option<String> {
    let base = format!("{}.{id}", crate::advertise::RULE_SYNC);
    let value: String = (0..crate::advertise::SYNC_RULES)
        .map_while(|i| rule(reply, &crate::advertise::part_name(&base, i)))
        .collect();
    (!value.is_empty()).then_some(value)
}

/// The join values a server advertises for each of the mods `ids`.
///
/// # Examples
///
/// ```
/// # fn reply(rules: &[(&str, &str)]) -> Vec<u8> {
/// #     let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 0, 0];
/// #     for (key, value) in rules {
/// #         reply.extend_from_slice(key.as_bytes());
/// #         reply.push(0);
/// #         reply.extend_from_slice(value.as_bytes());
/// #         reply.push(0);
/// #     }
/// #     reply
/// # }
/// use enshwick::join::server_values;
///
/// let reply = reply(&[("enshwick.sync.safes", "{ cost_multiplier = 2 }")]);
/// let values = server_values(&reply, ["safes", "camera"]);
/// assert_eq!(values.len(), 1);
/// ```
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
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
///
/// use enshwick::join::changes;
///
/// let incoming = BTreeMap::from([("safes".to_owned(), "{ cost_multiplier = 2 }".to_owned())]);
/// let changed = changes([("safes", None), ("", None)], &incoming);
/// assert_eq!(changed, [(0, Some("{ cost_multiplier = 2 }".to_owned()))]);
/// ```
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

/// The bytes of a join target the check reads, up to its address flag.
///
/// # Examples
///
/// ```
/// use enshwick::join::{HAS_ADDRESS, TARGET_BYTES};
///
/// assert_eq!(TARGET_BYTES, HAS_ADDRESS + 1);
/// ```
pub const TARGET_BYTES: usize = HAS_ADDRESS + 1;
/// The start of the join target the game hands its online provider, up to
/// the flag that its address is set (client builds 410 and 426). A join into
/// a hosted game names the lobby and no address.
///
/// # Examples
///
/// ```
/// use enshwick::join::{Destination, TARGET_BYTES, Target, destination};
///
/// let lobby: Target = [0; TARGET_BYTES];
/// assert_eq!(destination(&lobby), Destination::Lobby);
/// ```
pub type Target = [u8; TARGET_BYTES];
/// Where a join target holds the host's Steam id: the dedicated server's, or
/// the hosted game's lobby.
pub const HOST: usize = 0;
/// Where a join target holds the server's IPv4, as a little-endian `u32`.
pub const IP: usize = 8;
/// Where a join target holds the server's game port.
pub const GAME_PORT: usize = 0xc;
/// Where a join target holds the server's Steam query port.
pub const QUERY_PORT: usize = 0xe;
/// Where a join target holds the flag, 1, that its address is set.
pub const HAS_ADDRESS: usize = 0x20;
/// Where a join target holds the password, a string object of the game's.
pub const PASSWORD: usize = 0x24;
const _: () = assert!(
    TARGET_BYTES <= PASSWORD,
    "the check reads no byte of the password"
);

/// Where a join leads, read from its target as the game reads it.
///
/// # Examples
///
/// ```
/// use enshwick::join::{Destination, GAME_PORT, HAS_ADDRESS, IP, TARGET_BYTES, destination};
///
/// let mut target = [0; TARGET_BYTES];
/// target[IP..IP + 4].copy_from_slice(&0x7f00_0001_u32.to_le_bytes());
/// target[GAME_PORT..GAME_PORT + 2].copy_from_slice(&15646_u16.to_le_bytes());
/// target[HAS_ADDRESS] = 1;
/// assert_eq!(
///     destination(&target),
///     Destination::NoQueryPort { game: "127.0.0.1:15646".parse()? }
/// );
/// # Ok::<(), std::net::AddrParseError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Destination {
    /// A hosted game's lobby: the target sets no address and game port.
    Lobby,
    /// A dedicated server, whose rules are asked at `query`.
    Server {
        /// The server's address with its query port.
        query: SocketAddr,
    },
    /// A dedicated server whose target names no query port.
    NoQueryPort {
        /// The server's address with its game port.
        game: SocketAddr,
    },
}

/// Reads where a join target leads: a dedicated server when its address is
/// set with an IPv4 and a game port, as the game decides, else a lobby.
///
/// # Examples
///
/// ```
/// use enshwick::join::{
///     Destination, GAME_PORT, HAS_ADDRESS, IP, QUERY_PORT, TARGET_BYTES, destination,
/// };
///
/// let mut target = [0; TARGET_BYTES];
/// target[IP..IP + 4].copy_from_slice(&0x7f00_0001_u32.to_le_bytes());
/// target[GAME_PORT..GAME_PORT + 2].copy_from_slice(&15646_u16.to_le_bytes());
/// target[QUERY_PORT..QUERY_PORT + 2].copy_from_slice(&15647_u16.to_le_bytes());
/// target[HAS_ADDRESS] = 1;
/// assert_eq!(
///     destination(&target),
///     Destination::Server { query: "127.0.0.1:15647".parse()? }
/// );
/// # Ok::<(), std::net::AddrParseError>(())
/// ```
#[must_use]
pub fn destination(target: &Target) -> Destination {
    let ip = std::net::Ipv4Addr::from(u32::from_le_bytes(field(target, IP)));
    let game = u16::from_le_bytes(field(target, GAME_PORT));
    let query = u16::from_le_bytes(field(target, QUERY_PORT));
    if target[HAS_ADDRESS] != 1 || ip.is_unspecified() || game == 0 {
        Destination::Lobby
    } else if query == 0 {
        Destination::NoQueryPort {
            game: (ip, game).into(),
        }
    } else {
        Destination::Server {
            query: (ip, query).into(),
        }
    }
}

/// The `N` bytes of a join target from `at`, inside the target.
fn field<const N: usize>(target: &Target, at: usize) -> [u8; N] {
    core::array::from_fn(|i| target[at + i])
}

/// Takes the host out of a join target, so the game's own join step fails
/// on an invalid host and goes back to the menu, and returns the host to put
/// back once the game has copied the target.
///
/// # Examples
///
/// ```
/// use enshwick::join::{HOST, TARGET_BYTES, refuse};
///
/// let mut target = [0; TARGET_BYTES];
/// target[HOST..HOST + 8].copy_from_slice(&7_u64.to_le_bytes());
/// assert_eq!(refuse(&mut target), 7);
/// assert_eq!(target, [0; TARGET_BYTES]);
/// ```
#[must_use]
pub fn refuse(target: &mut Target) -> u64 {
    let host = u64::from_le_bytes(field(target, HOST));
    target[HOST..HOST + 8].fill(0);
    host
}

/// The value of `key` in an `A2S_RULES` reply.
///
/// # Examples
///
/// ```
/// # fn reply(rules: &[(&str, &str)]) -> Vec<u8> {
/// #     let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 0, 0];
/// #     for (key, value) in rules {
/// #         reply.extend_from_slice(key.as_bytes());
/// #         reply.push(0);
/// #         reply.extend_from_slice(value.as_bytes());
/// #         reply.push(0);
/// #     }
/// #     reply
/// # }
/// use enshwick::join::rule;
///
/// let reply = reply(&[("enshwick", "e5db5dca")]);
/// assert_eq!(rule(&reply, "enshwick").as_deref(), Some("e5db5dca"));
/// ```
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

/// Asks `server` for its rules over the Steam query protocol, giving up
/// two seconds after it starts, however the server answers.
///
/// # Errors
///
/// When the server does not answer in time, or not with rules.
///
/// # Examples
///
/// ```no_run
/// use enshwick::join::{query_rules, rule};
///
/// let reply = query_rules("127.0.0.1:15647".parse()?)?;
/// let fingerprint = rule(&reply, "enshwick");
/// # drop(fingerprint);
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn query_rules(server: SocketAddr) -> std::io::Result<Vec<u8>> {
    query_rules_within(server, QUERY_TIMEOUT)
}

/// [`query_rules`] with the whole query bound to `timeout`.
fn query_rules_within(server: SocketAddr, timeout: Duration) -> std::io::Result<Vec<u8>> {
    let deadline = Instant::now() + timeout;
    let socket = UdpSocket::bind(if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })?;
    socket.connect(server)?;
    let recv = |reply: &mut [u8]| {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(std::io::Error::from(std::io::ErrorKind::TimedOut));
        }
        socket.set_read_timeout(Some(left))?;
        socket.recv(reply)
    };
    let mut request = vec![0xff, 0xff, 0xff, 0xff, b'V', 0xff, 0xff, 0xff, 0xff];
    // A reply larger than the buffer fails to read on Windows, which would
    // let the join through unchecked.
    let mut reply = vec![0u8; 65_536];
    for _ in 0..3 {
        socket.send(&request)?;
        let len = recv(&mut reply)?;
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
                        let len = recv(&mut reply)?;
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

    /// The online provider's join: every join from the game's menus, to a
    /// dedicated server or into a hosted game's lobby, passes a target
    /// ([`super::Target`]) through it, and it ends in the provider's state
    /// change, whose `bool` it returns in `al` (client builds 410 and 426).
    pub(crate) type JoinGame = unsafe extern "system" fn(provider: *mut u8, target: *mut u8) -> u8;

    // SAFETY: the slot joins the chain of the join, typed `JoinGame`.
    /// The rest of the chain after the loader's link.
    pub(crate) static NEXT: Next<JoinGame> = unsafe { Next::new() };

    static NAMES: OnceLock<super::Names> = OnceLock::new();

    /// Sets the names of all the client's mods.
    pub(crate) fn set_names(names: super::Names) {
        let _first = NAMES.set(names);
    }

    /// The names of all the client's mods.
    pub(crate) fn names() -> super::Names {
        NAMES.get().cloned().unwrap_or_default()
    }

    /// The loader's link on the online provider's join.
    pub(crate) unsafe extern "system" fn detour(provider: *mut u8, target: *mut u8) -> u8 {
        let allowed = target.is_null() || {
            // SAFETY: the game's join target holds at least `TARGET_BYTES`
            // of plain data (client builds 410 and 426).
            let head = unsafe { target.cast::<super::Target>().read_unaligned() };
            std::panic::catch_unwind(|| check(&head)).unwrap_or(true)
        };
        if allowed {
            super::interaction::joining(false);
            // SAFETY: forwarded unchanged.
            return unsafe { NEXT.get()(provider, target) };
        }
        crate::log!(WARN, "join refused: the server runs mods this client lacks");
        super::interaction::joining(true);
        // The provider copies the target into its own state before it
        // returns, and the caller keeps its target, so the caller's copy
        // gets its host back (client builds 410 and 426).
        // SAFETY: the target is the caller's join target, writable for the
        // length of this call and not null; the reference ends here, before
        // the game reads the target.
        let host = super::refuse(unsafe { &mut *target.cast::<super::Target>() });
        // SAFETY: forwarded with the host taken out.
        let state_changed = unsafe { NEXT.get()(provider, target) };
        // SAFETY: the caller's target outlives this call, which its caller
        // makes with the target in hand, and the game holds no reference to
        // it once the provider returned (client builds 410 and 426); its
        // host id is an unaligned u64 at `HOST`.
        unsafe {
            target
                .wrapping_add(super::HOST)
                .cast::<u64>()
                .write_unaligned(host);
        }
        state_changed
    }

    /// Joins a game whose mods the check cannot read, on the client's own
    /// join values.
    fn unchecked() -> bool {
        crate::net::joining(false);
        crate::host::apply_join_values(&BTreeMap::new());
        true
    }

    /// The address to ask a join's server at, `None` for a join the check
    /// cannot ask about, with the reason logged.
    fn query_address(target: &super::Target) -> Option<std::net::SocketAddr> {
        match super::destination(target) {
            super::Destination::Server { query } => Some(query),
            super::Destination::Lobby => {
                // A hosted game's lobby: the lobby module reads its host's
                // mods once the client enters it.
                crate::log!(INFO, "joining a hosted game's lobby");
                None
            }
            super::Destination::NoQueryPort { game } => {
                crate::log!(
                    WARN,
                    "joining {game}: the game names no query port, so the join check cannot ask the server"
                );
                None
            }
        }
    }

    /// Compares mod sets, turns off what the server lacks, takes the server's
    /// join values and tells the player; false when the client must not join.
    fn check(target: &super::Target) -> bool {
        let Some(own_names) = NAMES.get() else {
            return true;
        };
        let Some(address) = query_address(target) else {
            return unchecked();
        };
        // Read now: a mod an earlier server or a failed hook turned off no
        // longer counts as the client's.
        let mods = &crate::host::join_mods();
        let reply = super::query_rules(address).ok();
        let Some(reply) = reply else {
            // A server that does not answer the query: nothing to compare.
            crate::log!(INFO, "joining {address}: no rules answer");
            return unchecked();
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
        let complete = super::mods_complete(&reply);
        if !complete {
            crate::log!(
                WARN,
                "joining {address}: the server lists only part of its mods; mods it does not list stay on"
            );
        }
        let plan = super::plan(
            &ours,
            &ours_mods,
            server.as_ref().map(|(f, m)| (f.as_str(), m.as_str())),
            complete,
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
        if plan.joins() {
            if let Some(note) = plan.note(&names) {
                // The join goes on, and the note shows over the game.
                crate::ui::show("Enshwick", &note, "", 1 << UiDialog::OK, |_| {});
            }
            return true;
        }
        refused(&plan.missing, &reply, &names);
        false
    }

    /// Tells the player, in one dialog, which of the server's mods keep the
    /// client out, and tells the client's mods.
    #[cold]
    fn refused(missing: &[super::Need], reply: &[u8], names: &super::Names) {
        let off = super::refusal::Off {
            by_player: crate::modlist::disabled().to_vec(),
            this_game: crate::host::join_mods_off(),
        };
        let pages: BTreeMap<String, String> = missing
            .iter()
            .filter_map(|need| Some((need.id.clone(), super::page(reply, &need.id)?)))
            .collect();
        let dialog = super::refusal::dialog(missing, &off, names, &pages);
        // Mods that are off on the client are the player's to turn on.
        let wanted: Vec<crate::host::Wanted> = missing
            .iter()
            .filter(|need| !off.by_player.contains(&need.id) && !off.this_game.contains(&need.id))
            .map(|need| crate::host::Wanted {
                name: super::name(reply, &need.id),
                page: pages.get(&need.id).cloned().unwrap_or_default(),
                id: need.id.clone(),
                version: need.version.clone(),
            })
            .collect();
        // The join is refused at once, so the game keeps drawing; the mods
        // hear of it once the player closes the dialog.
        crate::ui::show(
            &dialog.title,
            &dialog.message,
            &dialog.detail,
            1 << UiDialog::OK,
            move |_| crate::host::report_join_missing(&wanted),
        );
    }
}

#[cfg(windows)]
pub(crate) use runtime::{names, set_names};

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

    /// A server that sends a part of its reply just before every read times
    /// out, and never the rest, is given up on by the query's one deadline.
    #[test]
    fn a_reply_trickled_in_parts_ends_at_the_query_s_deadline() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let trickle = std::thread::spawn(move || {
            let mut request = [0u8; 64];
            let Ok((_, client)) = server.recv_from(&mut request) else {
                return;
            };
            let mut part = SPLIT.to_vec();
            part.extend_from_slice(&[1, 0, 0, 0, 2, 0, 0xe0, 0x04, 0xff]);
            for _ in 0..40 {
                if server.send_to(&part, client).is_err() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
        let started = Instant::now();
        let result = query_rules_within(address, Duration::from_millis(300));
        let took = started.elapsed();
        assert!(result.is_err());
        assert!(took < Duration::from_millis(900), "took {took:?}");
        drop(trickle);
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
        assert!(mods_complete(&reply));
    }

    #[test]
    fn a_cut_mod_list_turns_off_none_of_ours_it_does_not_name() {
        let names = Names::new();
        let cut = reply(&[
            ("enshwick", "cd"),
            ("enshwick.mods", "a@1"),
            ("enshwick.mods.count", "3"),
        ]);
        assert!(!mods_complete(&cut));
        let plan_cut = plan("ab", "a@1,z@1", Some(("cd", "a@1")), false);
        assert!(plan_cut.turn_off.is_empty() && plan_cut.joins());
        assert_eq!(plan_cut.note(&names), None);
        let whole = reply(&[
            ("enshwick", "cd"),
            ("enshwick.mods", "a@1,b@1,c@1"),
            ("enshwick.mods.count", "3"),
        ]);
        assert!(mods_complete(&whole));
        let plan_whole = plan("ab", "a@1,z@1", Some(("cd", "a@1")), true);
        assert_eq!(plan_whole.turn_off, ["z"]);
        // A server without a cut list sends no count.
        assert!(mods_complete(&reply(&[("enshwick.mods", "a@1")])));
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
    fn pages_are_read_from_the_rules() {
        let reply = reply(&[
            ("enshwick.page.a", "https://github.com/themaiby/enshwick"),
            ("enshwick.page.b", "github:themaiby/enshwick:enshwick-b"),
        ]);
        assert_eq!(
            page(&reply, "a").as_deref(),
            Some("https://github.com/themaiby/enshwick")
        );
        assert_eq!(page(&reply, "b"), None);
        assert_eq!(page(&reply, "c"), None);
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
        let missing = plan("ab", "", Some(("cd", server)), true);
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
                (false, "Dogs: yours 3.0.0, not on the server".to_owned()),
            ]
        );
        assert!(differences("", "", &names).is_empty());
    }

    /// A join target as the game's server list hands it: host id, address,
    /// game port, query port, and the flag that the address is set.
    fn target(
        host: u64,
        ip: [u8; 4],
        game_port: u16,
        query_port: u16,
        has_address: bool,
    ) -> Target {
        let mut target = [0u8; TARGET_BYTES];
        target[HOST..HOST + 8].copy_from_slice(&host.to_le_bytes());
        target[IP..IP + 4].copy_from_slice(&u32::from_be_bytes(ip).to_le_bytes());
        target[GAME_PORT..GAME_PORT + 2].copy_from_slice(&game_port.to_le_bytes());
        target[QUERY_PORT..QUERY_PORT + 2].copy_from_slice(&query_port.to_le_bytes());
        target[HAS_ADDRESS] = u8::from(has_address);
        target
    }

    #[test]
    fn a_dedicated_server_is_asked_at_its_query_port() {
        let target = target(90_295_777_516_953_619, [127, 0, 0, 1], 15646, 15647, true);
        assert_eq!(
            destination(&target),
            Destination::Server {
                query: "127.0.0.1:15647".parse().unwrap()
            }
        );
    }

    #[test]
    fn a_lobby_join_is_one_without_an_address_and_game_port() {
        let lobby = target(109_775_242_277_948_854, [0; 4], 0, 0, false);
        assert_eq!(destination(&lobby), Destination::Lobby);
        let unset = target(1, [10, 0, 0, 2], 15646, 15647, false);
        assert_eq!(destination(&unset), Destination::Lobby);
        let no_ip = target(1, [0; 4], 15646, 15647, true);
        assert_eq!(destination(&no_ip), Destination::Lobby);
        let no_game_port = target(1, [10, 0, 0, 2], 0, 15647, true);
        assert_eq!(destination(&no_game_port), Destination::Lobby);
    }

    #[test]
    fn a_dedicated_server_without_a_query_port_is_no_lobby() {
        let target = target(1, [10, 0, 0, 2], 15646, 0, true);
        assert_eq!(
            destination(&target),
            Destination::NoQueryPort {
                game: "10.0.0.2:15646".parse().unwrap()
            }
        );
    }

    #[test]
    fn a_refused_join_keeps_the_address_and_drops_the_host() {
        let mut target = target(90_295_777_516_953_619, [127, 0, 0, 1], 15646, 15647, true);
        let kept = target;
        let host = refuse(&mut target);
        assert_eq!(host, 90_295_777_516_953_619);
        assert_eq!(target[HOST..HOST + 8], [0; 8]);
        assert_eq!(target[IP..], kept[IP..]);
        assert_eq!(destination(&target), destination(&kept));
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
