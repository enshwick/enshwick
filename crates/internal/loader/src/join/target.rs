//! The join target the game hands its online provider, and where it leads.

use std::net::SocketAddr;

/// The bytes of a join target the check reads, up to its address flag.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::{HAS_ADDRESS, TARGET_BYTES};
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
/// use enshwick_loader::join::{Destination, TARGET_BYTES, Target, destination};
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
/// use enshwick_loader::join::{Destination, GAME_PORT, HAS_ADDRESS, IP, TARGET_BYTES, destination};
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
/// use enshwick_loader::join::{
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
/// use enshwick_loader::join::{HOST, TARGET_BYTES, refuse};
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
