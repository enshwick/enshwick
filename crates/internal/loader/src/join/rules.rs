//! The server's `enshwick` rules, asked over the Steam query protocol and
//! read.

use std::collections::BTreeMap;
use std::net::{SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use super::plan::entries;

/// The longest mod name a server may advertise, in characters.
const MAX_NAME: usize = 64;

/// The check holds the join, so a server that does not answer is not waited on long.
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);

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
/// use enshwick_loader::join::page;
///
/// let reply = reply(&[("enshwick.page.you.trading-post", "https://github.com/enshwick/enshwick")]);
/// assert!(page(&reply, "you.trading-post").is_some());
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
/// use enshwick_loader::join::name;
///
/// let reply = reply(&[("enshwick.name.you.trading-post", "Trading Post")]);
/// assert_eq!(name(&reply, "you.trading-post"), "Trading Post");
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
/// use enshwick_loader::join::mod_rules_value;
///
/// let reply = reply(&[("enshwick.mods", "you.trading-post@1.0.0"), ("enshwick.mods.1", "you.camera@1.0.0")]);
/// assert_eq!(mod_rules_value(&reply), "you.trading-post@1.0.0,you.camera@1.0.0");
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
/// use enshwick_loader::join::mods_complete;
///
/// let cut = reply(&[("enshwick.mods", "you.trading-post@1.0.0"), ("enshwick.mods.count", "2")]);
/// assert!(!mods_complete(&cut));
/// assert!(mods_complete(&reply(&[("enshwick.mods", "you.trading-post@1.0.0")])));
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
/// use enshwick_loader::join::sync_value;
///
/// let reply = reply(&[("enshwick.sync.you.trading-post", "{ cost_multiplier = 2 }")]);
/// assert_eq!(sync_value(&reply, "you.trading-post").as_deref(), Some("{ cost_multiplier = 2 }"));
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
/// use enshwick_loader::join::server_values;
///
/// let reply = reply(&[("enshwick.sync.you.trading-post", "{ cost_multiplier = 2 }")]);
/// let values = server_values(&reply, ["you.trading-post", "you.camera"]);
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

/// The mods whose join values change on joining a server with `incoming`, from
/// each mod's id (empty for a mod that does not join) and the server values it
/// runs with now.
///
/// The mod's index and the values it runs with next, `None` for its settings
/// file's.
///
/// # Examples
///
/// ```
/// use std::collections::BTreeMap;
///
/// use enshwick_loader::join::changes;
///
/// let incoming = BTreeMap::from([("you.trading-post".to_owned(), "{ cost_multiplier = 2 }".to_owned())]);
/// let changed = changes([("you.trading-post", None), ("", None)], &incoming);
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
/// use enshwick_loader::join::rule;
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
/// use enshwick_loader::join::{query_rules, rule};
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::join::{Names, plan, wanted};

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
            "you.a@1,you.b@1",
            "enshwick.mods.1",
            "you.c@1",
            "enshwick",
            "x",
        ] {
            reply.extend_from_slice(part.as_bytes());
            reply.push(0);
        }
        assert_eq!(mod_rules_value(&reply), "you.a@1,you.b@1,you.c@1");
        assert!(mods_complete(&reply));
    }

    #[test]
    fn a_cut_mod_list_turns_off_none_of_ours_it_does_not_name() {
        let names = Names::new();
        let cut = reply(&[
            ("enshwick", "cd"),
            ("enshwick.mods", "you.a@1"),
            ("enshwick.mods.count", "3"),
        ]);
        assert!(!mods_complete(&cut));
        let plan_cut = plan("ab", "you.a@1,you.z@1", Some(("cd", "you.a@1")), false);
        assert!(plan_cut.turn_off.is_empty() && plan_cut.joins());
        assert_eq!(plan_cut.note(&names), None);
        let whole = reply(&[
            ("enshwick", "cd"),
            ("enshwick.mods", "you.a@1,you.b@1,you.c@1"),
            ("enshwick.mods.count", "3"),
        ]);
        assert!(mods_complete(&whole));
        let plan_whole = plan("ab", "you.a@1,you.z@1", Some(("cd", "you.a@1")), true);
        assert_eq!(plan_whole.turn_off, ["you.z"]);
        // A server without a cut list sends no count.
        assert!(mods_complete(&reply(&[("enshwick.mods", "you.a@1")])));
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
        let advertised = crate::advertise::sync_rules("you.count", &"x".repeat(200)).unwrap();
        let mut rules: Vec<(&str, &str)> = advertised
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        rules.push(("enshwick.sync.you.zones", "{ a = 1 }"));
        rules.push(("enshwick.sync.gone", ""));
        let reply = reply(&rules);
        assert_eq!(sync_value(&reply, "you.count"), Some("x".repeat(200)));
        assert_eq!(sync_value(&reply, "gone"), None);
        assert_eq!(sync_value(&reply, "missing"), None);
        assert_eq!(
            server_values(&reply, ["you.zones", "gone", "you.other"]),
            BTreeMap::from([("you.zones".to_owned(), "{ a = 1 }".to_owned())])
        );
    }

    #[test]
    fn only_mods_whose_join_values_change_are_told() {
        let incoming = BTreeMap::from([
            ("you.a".to_owned(), "{ x = 1 }".to_owned()),
            ("you.b".to_owned(), "{ y = 2 }".to_owned()),
        ]);
        let current = [
            ("you.a", Some("{ x = 1 }")),
            ("you.b", None),
            ("you.c", Some("{ z = 3 }")),
            ("", None),
            ("you.d", None),
        ];
        assert_eq!(
            changes(current, &incoming),
            [(1, Some("{ y = 2 }".to_owned())), (2, None)]
        );
        assert!(changes([("you.a", None)], &BTreeMap::new()).is_empty());
    }

    #[test]
    fn pages_are_read_from_the_rules() {
        let reply = reply(&[
            (
                "enshwick.page.you.a",
                "https://github.com/enshwick/enshwick",
            ),
            ("enshwick.page.you.b", "github:enshwick/enshwick:enshwick-b"),
        ]);
        assert_eq!(
            page(&reply, "you.a").as_deref(),
            Some("https://github.com/enshwick/enshwick")
        );
        assert_eq!(page(&reply, "you.b"), None);
        assert_eq!(page(&reply, "you.c"), None);
    }

    #[test]
    fn a_server_s_ids_versions_and_names_are_checked_before_anyone_reads_them() {
        let server = "you.ok@1.2.0,../evil@1.0.0,x\ny@1.0.0,Caps@1.0.0,you.pack@0.1.0+ab12,you.bad@1.0\nInstall,@1";
        assert_eq!(
            wanted(server, ""),
            [
                ("you.ok".to_owned(), "1.2.0".to_owned()),
                ("you.pack".to_owned(), "0.1.0+ab12".to_owned())
            ]
        );
        let missing = plan("ab", "", Some(("cd", server)), true);
        assert_eq!(missing.missing.len(), 2, "{:?}", missing.missing);
        let reply = reply(&[
            (
                "enshwick.name.you.ok",
                "Trading Post\n\nVerified by Enshwick",
            ),
            ("enshwick.name.long", &"n".repeat(500)),
        ]);
        assert_eq!(name(&reply, "you.ok"), "Trading PostVerified by Enshwick");
        assert_eq!(name(&reply, "long").chars().count(), MAX_NAME);
    }

    #[test]
    fn a_rule_is_found_in_a_rules_reply() {
        let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 2, 0];
        for part in ["enshwick", "989b6a71", "enshwick.mods", "you.a@1"] {
            reply.extend_from_slice(part.as_bytes());
            reply.push(0);
        }
        assert_eq!(rule(&reply, "enshwick").as_deref(), Some("989b6a71"));
        assert_eq!(rule(&reply, "enshwick.mods").as_deref(), Some("you.a@1"));
        assert_eq!(rule(&reply, "missing"), None);
    }
}
