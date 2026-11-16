//! A server's `A2S_INFO` answer: the Steam id and game port a join needs
//! besides the query port, and whether the server is full or asks for a
//! password.

use std::net::SocketAddr;
use std::time::Duration;

/// A server's answer to `A2S_INFO`, what a join reads of it.
///
/// # Examples
///
/// ```
/// use enshwick_loader::base::join::ServerInfo;
///
/// let info = ServerInfo::default();
/// assert!(info.steam_id.is_none() && !info.full());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ServerInfo {
    /// The server's Steam id, which the game's join needs; `None` when the
    /// answer leaves it out.
    pub steam_id: Option<u64>,
    /// The port the game connects to; `None` when the answer leaves it out.
    pub game_port: Option<u16>,
    /// The players on the server.
    pub players: u8,
    /// The most players it takes.
    pub max_players: u8,
    /// Whether it asks for a password.
    pub password: bool,
}

impl ServerInfo {
    /// Whether the server had no room when it answered.
    #[must_use]
    pub const fn full(&self) -> bool {
        self.max_players != 0 && self.players >= self.max_players
    }
}

/// How long a join waits for the server's answer: the game's own join waits
/// longer, so this only decides that no answer comes.
const INFO_TIMEOUT: Duration = Duration::from_secs(3);

/// `A2S_INFO`'s request, before any challenge.
const REQUEST: &[u8] = b"\xff\xff\xff\xffTSource Engine Query\0";

/// The reply kind of `A2S_INFO`.
const KIND: u8 = b'I';

/// The extra data flags of an `A2S_INFO` answer, in the order of the
/// fields they announce.
mod extra {
    /// The game port, a `u16`.
    pub(super) const PORT: u8 = 0x80;
    /// The server's Steam id, a `u64`.
    pub(super) const STEAM_ID: u8 = 0x10;
}

/// Asks `server` at its query port for its `A2S_INFO`.
///
/// # Errors
///
/// When the server does not answer in time, or not with its info.
///
/// # Examples
///
/// ```no_run
/// use enshwick_loader::base::join::query_info;
///
/// let info = query_info("127.0.0.1:15637".parse()?)?;
/// assert!(info.steam_id.is_some());
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
pub fn query_info(server: SocketAddr) -> std::io::Result<ServerInfo> {
    let reply = super::rules::query_within(server, REQUEST, &[], KIND, INFO_TIMEOUT)?;
    parse_info(&reply).ok_or_else(|| std::io::Error::other("an info reply cut short"))
}

/// Reads an `A2S_INFO` reply, header included; `None` for one cut short.
///
/// # Examples
///
/// ```
/// use enshwick_loader::base::join::parse_info;
///
/// assert_eq!(parse_info(b"\xff\xff\xff\xffI"), None);
/// ```
#[must_use]
pub fn parse_info(reply: &[u8]) -> Option<ServerInfo> {
    let mut rest = reply.strip_prefix(&[0xff, 0xff, 0xff, 0xff, KIND])?;
    let mut take = |n: usize| -> Option<&[u8]> {
        let (head, tail) = rest.split_at_checked(n)?;
        rest = tail;
        Some(head)
    };
    // The protocol version.
    take(1)?;
    // Name, map, folder and game.
    for _ in 0..4 {
        skip_text(&mut take)?;
    }
    // The app id.
    take(2)?;
    let &[
        players,
        max_players,
        _bots,
        _kind,
        _environment,
        visibility,
        _vac,
    ] = take(7)?
    else {
        return None;
    };
    // The version.
    skip_text(&mut take)?;
    let mut info = ServerInfo {
        players,
        max_players,
        password: visibility != 0,
        ..ServerInfo::default()
    };
    let Some(&[flags]) = take(1) else {
        return Some(info);
    };
    if flags & extra::PORT != 0 {
        info.game_port = Some(u16::from_le_bytes(take(2)?.try_into().ok()?));
    }
    if flags & extra::STEAM_ID != 0 {
        info.steam_id = Some(u64::from_le_bytes(take(8)?.try_into().ok()?));
    }
    Some(info)
}

/// Skips a NUL-ended text through `take`; `None` when no NUL ends it.
fn skip_text<'a>(take: &mut impl FnMut(usize) -> Option<&'a [u8]>) -> Option<()> {
    while take(1)? != [0] {}
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An `A2S_INFO` reply as a Steam game server sends it, with `flags` and
    /// its fields after the version.
    fn reply(players: u8, max: u8, password: bool, flags: u8, extra: &[u8]) -> Vec<u8> {
        let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'I', 17];
        for text in ["A server", "map", "enshrouded", "Enshrouded"] {
            reply.extend_from_slice(text.as_bytes());
            reply.push(0);
        }
        reply.extend_from_slice(&1_203_620_u32.to_le_bytes()[..2]);
        reply.extend_from_slice(&[players, max, 0, b'd', b'w', u8::from(password), 0]);
        reply.extend_from_slice(b"1.0.0.0\0");
        reply.push(flags);
        reply.extend_from_slice(extra);
        reply
    }

    #[test]
    fn the_port_and_steam_id_read_from_the_extra_data() {
        let extra = [
            15636_u16.to_le_bytes().as_slice(),
            &90_295_777_516_953_619_u64.to_le_bytes(),
        ]
        .concat();
        let info = parse_info(&reply(
            3,
            16,
            true,
            extra::PORT | extra::STEAM_ID | 0x01,
            &extra,
        ))
        .unwrap();
        assert_eq!(info.game_port, Some(15636));
        assert_eq!(info.steam_id, Some(90_295_777_516_953_619));
        assert!(info.password && !info.full());
    }

    #[test]
    fn an_answer_without_extra_data_names_no_steam_id() {
        let mut short = reply(16, 16, false, 0, &[]);
        short.pop();
        let info = parse_info(&short).unwrap();
        assert_eq!((info.steam_id, info.game_port), (None, None));
        assert!(info.full() && !info.password);
    }

    #[test]
    fn a_reply_cut_inside_a_field_reads_as_none() {
        let whole = reply(1, 16, false, extra::PORT | extra::STEAM_ID, &[0; 10]);
        for cut in 5..whole.len() - 1 {
            let read = parse_info(&whole[..cut]);
            assert!(
                read.is_none() || read.as_ref().is_some_and(|i| i.steam_id.is_none()),
                "cut at {cut}: {read:?}"
            );
        }
        assert!(parse_info(&whole).unwrap().steam_id.is_some());
    }

    proptest::proptest! {
        #[test]
        fn any_reply_reads_without_panicking(bytes in proptest::collection::vec(proptest::num::u8::ANY, 0..96)) {
            let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'I'];
            reply.extend_from_slice(&bytes);
            let _read = parse_info(&reply);
        }
    }

    #[test]
    fn the_info_answer_of_a_server_on_this_computer_is_read() {
        let server = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        let address = server.local_addr().unwrap();
        let extra = [15636_u16.to_le_bytes().as_slice(), &7_u64.to_le_bytes()].concat();
        let answer = reply(0, 8, false, extra::PORT | extra::STEAM_ID, &extra);
        let serve = std::thread::spawn(move || {
            let mut request = [0u8; 64];
            let (len, client) = server.recv_from(&mut request).unwrap();
            assert_eq!(&request[..len], REQUEST);
            server
                .send_to(b"\xff\xff\xff\xffA\x01\x02\x03\x04", client)
                .unwrap();
            let (len, client) = server.recv_from(&mut request).unwrap();
            assert_eq!(&request[..len], [REQUEST, &[1, 2, 3, 4]].concat());
            server.send_to(&answer, client).unwrap();
        });
        let info = query_info(address).unwrap();
        serve.join().unwrap();
        assert_eq!((info.steam_id, info.game_port), (Some(7), Some(15636)));
    }
}
