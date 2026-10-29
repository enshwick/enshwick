//! The join target the game hands its online provider, and where it leads.

use std::net::SocketAddr;

use crate::pinned;

/// The most bytes from a join target's start the check copies.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::{TARGET_BYTES, Target};
///
/// let target: Target = [0; TARGET_BYTES];
/// assert!(target.iter().all(|&byte| byte == 0));
/// ```
pub const TARGET_BYTES: usize = 0x40; // bound, not a game offset: the loader's room for a target's start

/// The start of a join target the game hands its online provider, copied up
/// to its password; the bytes past [`Layout::head_len`] stay zero.
pub type Target = [u8; TARGET_BYTES];

/// Where a join target holds what the check reads, from the build table: the
/// host, the address, the ports and the flag that the address is set, all
/// before the password, which the check never reads.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::Layout;
///
/// let rows = [
///     ("join.target_host", 0),
///     ("join.target_copy", 0x50),
///     ("join.target_ip_read", 0x58),
///     ("join.target_word_copy", 0x48),
///     ("join.target_game_port_read", 0x4c),
///     ("join.target_query_port_read", 0x4e),
///     ("join.target_has_address", 32),
///     ("join.target_password", 36),
/// ];
/// let constant = |id: &str| rows.iter().find(|row| row.0 == id).map(|row| row.1);
/// let layout = Layout::from_table(constant)?;
/// assert_eq!(layout.head_len(), 36);
/// # Ok::<(), enshwick_loader::join::LayoutError>(())
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Layout {
    host: usize,
    ip: usize,
    game_port: usize,
    query_port: usize,
    has_address: usize,
    password: usize,
}

/// Why a build table's join target layout is not used: the join check stays
/// off.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::{Layout, LayoutError};
///
/// assert_eq!(
///     Layout::from_table(|_| None),
///     Err(LayoutError::Missing("join.target_password"))
/// );
/// ```
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LayoutError {
    /// The build table lacks a field of the target.
    #[error("the build table has no `{0}`")]
    Missing(&'static str),
    /// A field ends past the start of the password.
    #[error("`{0}` runs into the join target's password")]
    IntoPassword(&'static str),
    /// The join reads a field outside the stack copy it reads it from.
    #[error("`{0}` reads outside the join's copy of the target")]
    OutsideCopy(&'static str),
    /// The password starts past the bytes the check copies.
    #[error(
        "the join target's password at {0:#x} lies past the {TARGET_BYTES} bytes the check copies"
    )]
    TooLong(u64),
}

impl Layout {
    /// Reads the layout from a build table's `constant` lookup.
    ///
    /// # Errors
    ///
    /// [`LayoutError::Missing`] for a field the table lacks,
    /// [`LayoutError::IntoPassword`] for one that ends past the password's
    /// start, [`LayoutError::OutsideCopy`] for one the join reads outside
    /// its stack copy of the target, [`LayoutError::TooLong`] for a
    /// password past [`TARGET_BYTES`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::join::{Layout, LayoutError};
    ///
    /// let into_password = |id: &str| Some(if id == "join.target_password" { 4 } else { 0 });
    /// assert_eq!(
    ///     Layout::from_table(into_password),
    ///     Err(LayoutError::IntoPassword("join.target_host"))
    /// );
    /// ```
    pub fn from_table(constant: impl Fn(&str) -> Option<u64>) -> Result<Self, LayoutError> {
        use pinned::join::{
            TARGET_COPY, TARGET_GAME_PORT_READ, TARGET_HAS_ADDRESS, TARGET_HOST, TARGET_IP_READ,
            TARGET_PASSWORD, TARGET_QUERY_PORT_READ, TARGET_WORD_COPY,
        };
        let row = |id: &'static str| constant(id).ok_or(LayoutError::Missing(id));
        let password = row(TARGET_PASSWORD.id())?;
        let password = usize::try_from(password)
            .ok()
            .filter(|&at| at <= TARGET_BYTES)
            .ok_or(LayoutError::TooLong(password))?;
        let field = |id: &'static str, at: usize, size: usize| {
            Some(at)
                .filter(|at| at.checked_add(size).is_some_and(|end| end <= password))
                .ok_or(LayoutError::IntoPassword(id))
        };
        // The join reads the address from its stack copy of the target's
        // first 16 bytes, and the ports from a copy of the 8 bytes from the
        // address on.
        let copy = row(TARGET_COPY.id())?;
        let ip_read = TARGET_IP_READ.id();
        let ip = copied(ip_read, row(ip_read)?, size_of::<u32>(), copy, 16)?;
        let word = row(TARGET_WORD_COPY.id())?;
        let port = |id: &'static str| {
            copied(id, row(id)?, size_of::<u16>(), word, size_of::<u64>())
                .and_then(|at| ip.checked_add(at).ok_or(LayoutError::OutsideCopy(id)))
        };
        Ok(Self {
            host: field(
                TARGET_HOST.id(),
                usize_from(row(TARGET_HOST.id())?),
                size_of::<u64>(),
            )?,
            ip: field(TARGET_IP_READ.id(), ip, size_of::<u32>())?,
            game_port: field(
                TARGET_GAME_PORT_READ.id(),
                port(TARGET_GAME_PORT_READ.id())?,
                size_of::<u16>(),
            )?,
            query_port: field(
                TARGET_QUERY_PORT_READ.id(),
                port(TARGET_QUERY_PORT_READ.id())?,
                size_of::<u16>(),
            )?,
            has_address: field(
                TARGET_HAS_ADDRESS.id(),
                usize_from(row(TARGET_HAS_ADDRESS.id())?),
                size_of::<u8>(),
            )?,
            password,
        })
    }

    /// How many bytes from a target's start the check reads: up to its
    /// password, at most [`TARGET_BYTES`].
    #[must_use]
    pub const fn head_len(&self) -> usize {
        self.password
    }

    /// Where a target holds its host's Steam id, a `u64`: the dedicated
    /// server's, or the hosted game's lobby.
    #[must_use]
    pub const fn host_at(&self) -> usize {
        self.host
    }

    /// Reads where a join target leads: a dedicated server when its address
    /// is set with an IPv4 and a game port, as the game decides, else a
    /// lobby.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::join::{Destination, Layout, TARGET_BYTES};
    ///
    /// # let rows = [
    /// #     ("join.target_host", 0),
    /// #     ("join.target_copy", 0x50),
    /// #     ("join.target_ip_read", 0x58),
    /// #     ("join.target_word_copy", 0x48),
    /// #     ("join.target_game_port_read", 0x4c),
    /// #     ("join.target_query_port_read", 0x4e),
    /// #     ("join.target_has_address", 32),
    /// #     ("join.target_password", 36),
    /// # ];
    /// # let layout = Layout::from_table(|id| rows.iter().find(|row| row.0 == id).map(|row| row.1))?;
    /// let lobby = [0; TARGET_BYTES];
    /// assert_eq!(layout.destination(&lobby), Destination::Lobby);
    /// # Ok::<(), enshwick_loader::join::LayoutError>(())
    /// ```
    #[must_use]
    pub fn destination(&self, target: &Target) -> Destination {
        let ip = std::net::Ipv4Addr::from(u32::from_le_bytes(field(target, self.ip)));
        let game = u16::from_le_bytes(field(target, self.game_port));
        let query = u16::from_le_bytes(field(target, self.query_port));
        if target[self.has_address] != 1 || ip.is_unspecified() || game == 0 {
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
}

/// Where a join leads, read from its target as the game reads it.
///
/// # Examples
///
/// ```
/// use enshwick_loader::join::Destination;
///
/// let server = Destination::Server { query: "127.0.0.1:15647".parse()? };
/// assert_ne!(server, Destination::Lobby);
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

/// A row's value as an offset; one past the address space lies past every
/// target.
fn usize_from(value: u64) -> usize {
    usize::try_from(value).unwrap_or(usize::MAX)
}

/// Where a `size`-byte field lies in the `copied` bytes the join copies to
/// its stack at `copy`, when it reads the field at `read` (row `id`);
/// [`LayoutError::OutsideCopy`] when the read misses the copy.
fn copied(
    id: &'static str,
    read: u64,
    size: usize,
    copy: u64,
    copied: usize,
) -> Result<usize, LayoutError> {
    read.checked_sub(copy)
        .and_then(|at| usize::try_from(at).ok())
        .filter(|at| at.checked_add(size).is_some_and(|end| end <= copied))
        .ok_or(LayoutError::OutsideCopy(id))
}

/// The `N` bytes of a join target from `at`, which [`Layout::from_table`]
/// keeps inside the target.
fn field<const N: usize>(target: &Target, at: usize) -> [u8; N] {
    core::array::from_fn(|i| target[at + i])
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The rows of client builds 410 and 426.
    const ROWS: [(&str, u64); 8] = [
        ("join.target_host", 0),
        ("join.target_copy", 0x50),
        ("join.target_ip_read", 0x58),
        ("join.target_word_copy", 0x48),
        ("join.target_game_port_read", 0x4c),
        ("join.target_query_port_read", 0x4e),
        ("join.target_has_address", 32),
        ("join.target_password", 36),
    ];

    fn rows(id: &str) -> Option<u64> {
        ROWS.iter().find(|row| row.0 == id).map(|row| row.1)
    }

    fn layout() -> Layout {
        Layout::from_table(rows).unwrap()
    }

    fn with(id: &'static str, value: u64) -> impl Fn(&str) -> Option<u64> {
        move |asked| {
            if asked == id {
                Some(value)
            } else {
                rows(asked)
            }
        }
    }

    #[test]
    fn the_address_and_ports_lie_where_the_join_s_stack_copies_read_them() {
        let layout = layout();
        assert_eq!(
            (layout.ip, layout.game_port, layout.query_port),
            (8, 12, 14)
        );
    }

    #[test]
    fn a_read_outside_its_stack_copy_turns_the_check_off() {
        let outside = |id, value| Layout::from_table(with(id, value));
        assert_eq!(
            outside("join.target_ip_read", 0x4f),
            Err(LayoutError::OutsideCopy("join.target_ip_read"))
        );
        assert_eq!(
            outside("join.target_ip_read", 0x60),
            Err(LayoutError::OutsideCopy("join.target_ip_read"))
        );
        assert_eq!(
            outside("join.target_game_port_read", 0x47),
            Err(LayoutError::OutsideCopy("join.target_game_port_read"))
        );
        assert_eq!(
            outside("join.target_query_port_read", 0x4f),
            Err(LayoutError::OutsideCopy("join.target_query_port_read"))
        );
        assert_eq!(
            outside("join.target_copy", u64::MAX),
            Err(LayoutError::OutsideCopy("join.target_ip_read"))
        );
    }

    /// A join target as the game's server list hands it: host id, address,
    /// game port, query port, and the flag that the address is set.
    fn target(ip: [u8; 4], game_port: u16, query_port: u16, has_address: bool) -> Target {
        let layout = layout();
        let mut target = [0u8; TARGET_BYTES];
        let mut put = |at: usize, bytes: &[u8]| target[at..at + bytes.len()].copy_from_slice(bytes);
        put(layout.host, &90_295_777_516_953_619_u64.to_le_bytes());
        put(layout.ip, &u32::from_be_bytes(ip).to_le_bytes());
        put(layout.game_port, &game_port.to_le_bytes());
        put(layout.query_port, &query_port.to_le_bytes());
        put(layout.has_address, &[u8::from(has_address)]);
        target
    }

    #[test]
    fn a_dedicated_server_is_asked_at_its_query_port() {
        let target = target([127, 0, 0, 1], 15646, 15647, true);
        assert_eq!(
            layout().destination(&target),
            Destination::Server {
                query: "127.0.0.1:15647".parse().unwrap()
            }
        );
    }

    #[test]
    fn a_lobby_join_is_one_without_an_address_and_game_port() {
        let layout = layout();
        let lobby = target([0; 4], 0, 0, false);
        assert_eq!(layout.destination(&lobby), Destination::Lobby);
        let unset = target([10, 0, 0, 2], 15646, 15647, false);
        assert_eq!(layout.destination(&unset), Destination::Lobby);
        let no_ip = target([0; 4], 15646, 15647, true);
        assert_eq!(layout.destination(&no_ip), Destination::Lobby);
        let no_game_port = target([10, 0, 0, 2], 0, 15647, true);
        assert_eq!(layout.destination(&no_game_port), Destination::Lobby);
    }

    #[test]
    fn a_dedicated_server_without_a_query_port_is_no_lobby() {
        let target = target([10, 0, 0, 2], 15646, 0, true);
        assert_eq!(
            layout().destination(&target),
            Destination::NoQueryPort {
                game: "10.0.0.2:15646".parse().unwrap()
            }
        );
    }

    #[test]
    fn the_check_reads_up_to_the_password() {
        let layout = layout();
        assert_eq!(layout.head_len(), 36);
        assert_eq!(layout.host_at(), 0);
    }

    #[test]
    fn a_missing_row_turns_the_check_off() {
        for (id, _) in ROWS {
            let without = |asked: &str| (asked != id).then(|| rows(asked)).flatten();
            assert_eq!(Layout::from_table(without), Err(LayoutError::Missing(id)));
        }
    }

    #[test]
    fn a_field_that_reaches_the_password_turns_the_check_off() {
        let password = |id: &str| {
            if id == "join.target_password" {
                Some(32)
            } else {
                rows(id)
            }
        };
        assert_eq!(
            Layout::from_table(password),
            Err(LayoutError::IntoPassword("join.target_has_address"))
        );
    }

    #[test]
    fn a_password_past_the_copied_bytes_turns_the_check_off() {
        let far = TARGET_BYTES as u64 + 1;
        let password = |id: &str| {
            if id == "join.target_password" {
                Some(far)
            } else {
                rows(id)
            }
        };
        assert_eq!(Layout::from_table(password), Err(LayoutError::TooLong(far)));
    }

    /// On the client `ENSH_CLIENT_EXE` names (skipped without it), its table
    /// and every client table carried to it by patterns give the layout the
    /// check was written against.
    #[test]
    fn the_client_s_tables_give_the_join_target_layout() {
        let Ok(path) = std::env::var("ENSH_CLIENT_EXE") else {
            return;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = enshwick_gamedata::reflect::Image::parse(
            &bytes,
            enshwick_gamedata::reflect::Layout::File,
            None,
        )
        .unwrap();
        let tables = crate::builds::load();
        let fingerprint = image.fingerprint().unwrap();
        let own = enshwick_gamedata::exact(&tables, enshwick_gamedata::Side::Client, fingerprint)
            .unwrap()
            .resolve();
        assert_eq!(Layout::from_table(|id| own.constant(id)), Ok(layout()));
        let base = image.base();
        let text = image.range(image.section(".text").unwrap());
        let code = image.read(text.start, text.end - text.start).unwrap();
        for other in tables
            .iter()
            .filter(|t| t.side() == enshwick_gamedata::Side::Client)
        {
            let carried = other.fall_back(code, text.start - base, |rva| image.u32(base + rva));
            assert_eq!(
                Layout::from_table(|id| carried.constant(id)),
                Ok(layout()),
                "{}",
                other.name()
            );
        }
    }
}
