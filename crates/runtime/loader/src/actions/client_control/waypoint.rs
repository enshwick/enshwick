//! Where the local player's waypoint request lies in its
//! `ClientPlayerInput`: the input builder writes it from the map's own
//! waypoint and ping events, and the server sets, clears or pings when its
//! version changes (client 26430625 `0x1403af670`; server 26430253
//! `player_waypoint_selection`, read in the decompiler).

use crate::engine::catalogue::{self, ids};
use enshwick_core::ecs::fixed;

use super::{fits, reflected_size};
use crate::engine::loader::Loader;

/// The bit every version the loader writes sets. The game versions its
/// requests by the world's step count, which stays below it for over a
/// year of play at 60 steps a second, so a request of the game's never
/// carries the loader's version and the server takes both.
const LOADER_VERSION: u32 = 1 << 31;

/// What a command asks of the waypoint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    /// Sets or moves the waypoint to the position, in metres.
    Set([f64; 3]),
    /// Clears the waypoint.
    Clear,
    /// Pings the position, in metres, leaving the waypoint.
    Ping([f64; 3]),
}

/// The request's fields, at their offsets in `ClientPlayerInput`.
#[derive(Debug)]
pub(crate) struct WaypointLayout {
    position: usize,
    enable: usize,
    is_ping: usize,
    version: usize,
}

impl WaypointLayout {
    /// The layout, when the build reflects the request with every field
    /// inside it and the request inside the input.
    pub(super) fn resolve(loader: &Loader) -> Result<Self, &'static str> {
        let input = reflected_size(loader, ids::CLIENT_PLAYER_INPUT)?;
        let size = reflected_size(loader, ids::WAYPOINT_ACTION)?;
        let inside = |id: &'static str, bytes: usize| {
            catalogue::offset(loader, id)
                .filter(|&at| fits(at, bytes, size))
                .ok_or(id)
        };
        let base = catalogue::offset(loader, ids::CLIENT_PLAYER_INPUT_WAYPOINT)
            .filter(|&at| fits(at, size, input))
            .ok_or(ids::CLIENT_PLAYER_INPUT_WAYPOINT)?;
        Ok(Self {
            position: base + inside(ids::WAYPOINT_ACTION_POSITION, size_of::<[i64; 3]>())?,
            enable: base + inside(ids::WAYPOINT_ACTION_ENABLE, 1)?,
            is_ping: base + inside(ids::WAYPOINT_ACTION_IS_PING, 1)?,
            version: base + inside(ids::WAYPOINT_ACTION_VERSION, size_of::<u32>())?,
        })
    }

    /// Writes `mark` as a new request, its version moved on from the one
    /// the record holds, so the server takes it once.
    ///
    /// # Safety
    ///
    /// - `input` is valid for reads and writes of the build's
    ///   `ClientPlayerInput`, which nothing else touches during the call.
    pub(crate) unsafe fn write(&self, input: *mut u8, mark: Mark) {
        let (at, enable, ping) = match mark {
            Mark::Set(at) => (at, 1u8, 0u8),
            Mark::Clear => ([0.0; 3], 0, 0),
            // The game's own ping sets both bytes (client 26430625).
            Mark::Ping(at) => (at, 1, 1),
        };
        // SAFETY: the caller's record, which `resolve` checked holds the
        // request's `u32` version at `version`.
        let old: u32 = unsafe { enshwick_core::raw::read_at(input, self.version) };
        // SAFETY: the caller's record, which `resolve` checked holds the
        // request's position at `position`.
        unsafe { enshwick_core::raw::write_at(input, self.position, fixed(at)) };
        // SAFETY: the caller's record; its `bool` at `enable`, written as a
        // byte.
        unsafe { enshwick_core::raw::write_at(input, self.enable, enable) };
        // SAFETY: the caller's record; its `bool` at `is_ping`, written as a
        // byte.
        unsafe { enshwick_core::raw::write_at(input, self.is_ping, ping) };
        // SAFETY: the caller's record, its version at `version`.
        unsafe { enshwick_core::raw::write_at(input, self.version, next_version(old)) };
    }
}

/// The version after `old` that no request of the game's carries.
const fn next_version(old: u32) -> u32 {
    old.wrapping_add(1) | LOADER_VERSION
}

/// The layout of client 26430625, for tests over records of their own.
#[cfg(test)]
pub(crate) const TEST_LAYOUT: WaypointLayout = WaypointLayout {
    position: 0x238,
    enable: 0x250,
    is_ping: 0x251,
    version: 0x254,
};

#[cfg(test)]
mod tests {
    use super::*;

    fn at<T: bytemuck::AnyBitPattern>(input: &[u8], from: usize) -> T {
        bytemuck::pod_read_unaligned(&input[from..from + size_of::<T>()])
    }

    #[test]
    fn each_request_moves_the_version_on_where_no_game_request_reaches() {
        assert_eq!(next_version(0x29), 0x2a | LOADER_VERSION);
        assert_eq!(next_version(0x2a | LOADER_VERSION), 0x2b | LOADER_VERSION);
        assert_eq!(next_version(u32::MAX), LOADER_VERSION, "past the wrap");
        assert_ne!(next_version(LOADER_VERSION - 1), LOADER_VERSION - 1);
    }

    #[test]
    fn a_set_a_clear_and_a_ping_write_their_request_in_the_input() {
        let mut input = vec![0u8; 0x600];
        input[0x254..0x258].copy_from_slice(&7u32.to_ne_bytes());
        let request = |input: &mut [u8], mark| {
            // SAFETY: a record of the build's size, not shared.
            unsafe { TEST_LAYOUT.write(input.as_mut_ptr(), mark) };
        };
        request(&mut input, Mark::Set([1.5, -2.0, 3.25]));
        assert_eq!(
            at::<[i64; 3]>(&input, 0x238),
            [3 << 31, -(2 << 32), 13 << 30]
        );
        assert_eq!((input[0x250], input[0x251]), (1, 0));
        assert_eq!(at::<u32>(&input, 0x254), 0x8 | LOADER_VERSION);
        request(&mut input, Mark::Ping([4.0, 0.0, 4.0]));
        assert_eq!((input[0x250], input[0x251]), (1, 1));
        request(&mut input, Mark::Clear);
        assert_eq!((input[0x250], input[0x251]), (0, 0));
        assert_eq!(at::<u32>(&input, 0x254), 0xa | LOADER_VERSION);
    }
}
