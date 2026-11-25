//! The local player's own state on the client: where it stands and where
//! its camera looks, its health and stamina, and its action bar's selected
//! slot, read beside the held controls in `player_ui`'s entry, on the row
//! that read found, and kept for any thread to read.
//!
//! Every value is the client's replicated copy, the one its interface
//! draws: `player_ui` copies the same `NetworkPlayerAttributes` fields into
//! the HUD (client 26430625), so a value may trail the server by a step.

use crate::engine::copied::Copied;

/// The feature's name in the log and on the Mods screen.
pub(crate) const FEATURE: &str = "the local player's position, look, vitals and action bar";

/// The parts a step read, a bit each, as the snapshot's groups name them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Parts(u8);

impl Parts {
    pub(crate) const POSITION: Self = Self(1);
    pub(crate) const LOOK: Self = Self(2);
    pub(crate) const VITALS: Self = Self(4);
    pub(crate) const ACTIONBAR: Self = Self(8);

    pub(crate) const fn contains(self, part: Self) -> bool {
        self.0 & part.0 == part.0
    }

    #[must_use]
    pub(crate) const fn with(self, part: Self) -> Self {
        Self(self.0 | part.0)
    }
}

/// One step's read of the local player.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Read {
    /// The parts this read holds.
    pub(crate) parts: Parts,
    /// Its position, three 32.32 fixed-point `i64` metres.
    pub(crate) position: [i64; 3],
    /// Its camera's orientation, a quaternion x, y, z, w.
    pub(crate) look: [f32; 4],
    /// Health, maximum health, stamina and maximum stamina.
    pub(crate) vitals: [u16; 4],
    /// The action bar's selected slot.
    pub(crate) actionbar: u8,
}

impl Read {
    /// The read in words, with the time `at` it was taken, as
    /// [`local_held::now_ms`](super::local_held) counts.
    fn pack(&self, at: u64) -> [u64; 8] {
        let [x, y, z] = self.position.map(i64::cast_unsigned);
        let [l0, l1, l2, l3] = self.look.map(|part| u64::from(part.to_bits()));
        let [h, hm, s, sm] = self.vitals.map(u64::from);
        [
            x,
            y,
            z,
            l0 | l1 << 32,
            l2 | l3 << 32,
            h | hm << 16 | s << 32 | sm << 48,
            u64::from(self.actionbar) | u64::from(self.parts.0) << 8,
            at,
        ]
    }

    /// The read packed into `words`, and when it was taken.
    fn unpack(words: [u64; 8]) -> (Self, u64) {
        let [x, y, z, look01, look23, vitals, slot, at] = words;
        let low = |word: u64| f32::from_bits(u32::try_from(word & 0xffff_ffff).unwrap_or(0));
        let half = |shift: u32| u16::try_from(vitals >> shift & 0xffff).unwrap_or(0);
        let read = Self {
            parts: Parts(u8::try_from(slot >> 8 & 0xff).unwrap_or(0)),
            position: [x, y, z].map(u64::cast_signed),
            look: [
                low(look01),
                low(look01 >> 32),
                low(look23),
                low(look23 >> 32),
            ],
            vitals: [half(0), half(16), half(32), half(48)],
            actionbar: u8::try_from(slot & 0xff).unwrap_or(0),
        };
        (read, at)
    }
}

/// The last read, written only on `player_ui`'s thread.
static COPY: Copied<8> = Copied::new();

/// Keeps a step's `read`, taken at `at`.
fn keep(read: &Read, at: u64) {
    COPY.write(read.pack(at));
}

/// The last whole read and when it was taken; `None` before a step read
/// one, or while a write stays mid-way through every try.
pub(crate) fn last() -> Option<(Read, u64)> {
    let (read, at) = Read::unpack(COPY.load()?);
    (at != 0).then_some((read, at))
}

#[cfg(windows)]
pub(crate) use game::{FUSE, Layout, read};

#[cfg(windows)]
mod game {
    use enshwick_gamedata::catalogue;

    use super::{FEATURE, Parts, Read, keep};
    use crate::engine::ecs::Components;
    use crate::engine::fuse::Fuse;
    use crate::engine::loader::Loader;

    /// The switch of these reads alone, so a fault leaves the held controls
    /// they run beside read.
    pub(crate) static FUSE: Fuse = Fuse::new(FEATURE);

    /// Where each part reads on this build: a component's type hash and its
    /// field offsets; `None` for a part the build lacks.
    #[derive(Debug)]
    pub(crate) struct Layout {
        /// `CurrentTransform`'s hash and the offset of its position.
        position: Option<(u32, usize)>,
        /// The camera orientation's offset inside `ClientPlayerInput`.
        look: Option<usize>,
        /// `NetworkPlayerAttributes`' hash and its four fields' offsets.
        vitals: Option<(u32, [usize; 4])>,
        /// `SlotSelection`'s hash and the action bar index's offset.
        actionbar: Option<(u32, usize)>,
    }

    impl Layout {
        /// The parts `loader`'s build reads, each from its reflected names;
        /// a part the build lacks logs its id once and stays unread.
        pub(crate) fn resolve(loader: &Loader) -> Self {
            let at = |id: &'static str| {
                catalogue::constant(id, &loader.reflection).and_then(|at| usize::try_from(at).ok())
            };
            let hash = catalogue::type_hash;
            let [health, health_max, stamina, stamina_max] = [
                catalogue::ids::NETWORK_PLAYER_ATTRIBUTES_HEALTH,
                catalogue::ids::NETWORK_PLAYER_ATTRIBUTES_HEALTH_MAX,
                catalogue::ids::NETWORK_PLAYER_ATTRIBUTES_STAMINA,
                catalogue::ids::NETWORK_PLAYER_ATTRIBUTES_STAMINA_MAX,
            ]
            .map(at);
            let layout = Self {
                position: hash(catalogue::ids::CURRENT_TRANSFORM).zip(
                    at(catalogue::ids::CURRENT_TRANSFORM_TRANSFORM)
                        .zip(at(catalogue::ids::WORLD_TRANSFORM_POSITION))
                        .map(|(transform, position)| transform + position),
                ),
                look: at(catalogue::ids::CLIENT_PLAYER_INPUT_CAMERA_ORIENTATION),
                vitals: hash(catalogue::ids::NETWORK_PLAYER_ATTRIBUTES).zip(
                    health
                        .zip(health_max)
                        .zip(stamina.zip(stamina_max))
                        .map(|((h, hm), (s, sm))| [h, hm, s, sm]),
                ),
                actionbar: hash(catalogue::ids::SLOT_SELECTION)
                    .zip(at(catalogue::ids::SLOT_SELECTION_ACTIONBAR)),
            };
            for (lacks, part) in [
                (layout.position.is_none(), catalogue::ids::CURRENT_TRANSFORM),
                (
                    layout.look.is_none(),
                    catalogue::ids::CLIENT_PLAYER_INPUT_CAMERA_ORIENTATION,
                ),
                (
                    layout.vitals.is_none(),
                    catalogue::ids::NETWORK_PLAYER_ATTRIBUTES,
                ),
                (layout.actionbar.is_none(), catalogue::ids::SLOT_SELECTION),
            ] {
                if lacks {
                    crate::log_once!(WARN, "{FEATURE}: this build lacks {part}; it stays unread");
                }
            }
            layout
        }
    }

    /// Reads the local player `player`'s parts and keeps them, taken at
    /// `at`.
    ///
    /// # Safety
    ///
    /// - `ctx` is the `player_ui` system's live, non-null context, at its
    ///   entry on its thread.
    /// - `input` is `player`'s live `ClientPlayerInput`, unwritten until the
    ///   system runs.
    pub(crate) unsafe fn read(
        ctx: *mut u64,
        components: &Components,
        layout: &Layout,
        player: u32,
        input: *const u8,
        at: u64,
    ) {
        let mut read = Read::default();
        let component = |hash: u32| {
            // SAFETY: the caller's live context at the system's entry, on its
            // thread.
            unsafe { components.component(ctx, player, hash) }
        };
        if let Some((hash, position)) = layout.position
            && let Some(data) = component(hash)
        {
            // SAFETY: `data` is the player's live `CurrentTransform`, whose
            // position, three fixed-point `i64`, sits at the reflected offset.
            read.position = unsafe { enshwick_core::raw::read_at(data, position) };
            read.parts = read.parts.with(Parts::POSITION);
        }
        if let Some(look) = layout.look {
            // SAFETY: the caller's live `ClientPlayerInput`, whose camera
            // orientation, a reflected `Quaternion` of four `f32`, sits at
            // `look`.
            read.look = unsafe { enshwick_core::raw::read_at(input, look) };
            read.parts = read.parts.with(Parts::LOOK);
        }
        if let Some((hash, fields)) = layout.vitals
            && let Some(data) = component(hash)
        {
            // SAFETY: `data` is the player's live `NetworkPlayerAttributes`,
            // whose four fields are reflected `u16` at these offsets.
            read.vitals = fields.map(|at| unsafe { enshwick_core::raw::read_at(data, at) });
            read.parts = read.parts.with(Parts::VITALS);
        }
        if let Some((hash, index)) = layout.actionbar
            && let Some(data) = component(hash)
        {
            // SAFETY: `data` is the player's live `SlotSelection`, whose
            // action bar index is a reflected `u8` at `index`.
            read.actionbar = unsafe { enshwick_core::raw::read_at(data, index) };
            read.parts = read.parts.with(Parts::ACTIONBAR);
        }
        keep(&read, at);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_read_comes_back_whole_from_its_words() {
        let read = Read {
            parts: Parts::POSITION.with(Parts::VITALS).with(Parts::ACTIONBAR),
            position: [-5 << 32, 1 << 40, i64::MIN],
            look: [0.0, -0.707, 0.0, 0.707],
            vitals: [37, 120, 0, u16::MAX],
            actionbar: 7,
        };
        assert_eq!(Read::unpack(read.pack(42)), (read, 42));
    }

    #[test]
    fn a_part_counts_only_when_its_bit_is_set() {
        let parts = Parts::LOOK.with(Parts::ACTIONBAR);
        assert!(parts.contains(Parts::LOOK));
        assert!(!parts.contains(Parts::VITALS));
        assert!(!Parts::default().contains(Parts::POSITION));
    }
}
