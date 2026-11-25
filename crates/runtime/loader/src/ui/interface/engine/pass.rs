//! The keys the game lets through its modal windows, and the map's added to
//! them while a carried window points at the map.
//!
//! While a modal window shows, the game hands every key event to its
//! interface first, and the window's end takes the event, so it never
//! reaches the input mapping, unless the event is one of a list of keys it
//! lets through: the voice chat's, which the game rebuilds when its state
//! changes (`ui.window_end`, `ui.pass_refresh`, client 26430625, read in the
//! decompiler; the taking seen live). The loader adds the map
//! action's bound keys to that list, keyboard, mouse and gamepad alike, and
//! has the game rebuild its own once they are no longer wanted.

use core::sync::atomic::{AtomicBool, Ordering};

use enshwick_gamedata::catalogue;

use crate::actions::game_bindings::{ActionNumber, GameActions};
use crate::engine::interface_parts::Missing;
use crate::engine::loader::Loader;
use crate::engine::memory;
use crate::engine::pinned::{self, Constant, Symbol};

/// The game's name of the action that opens and closes the map, the
/// generated `GameplayAction::UiMap`'s (held by a test).
const MAP_ACTION: &str = "UiMap";

/// The most keys a list the loader hands the game holds: the game's own,
/// two per binding of each voice chat action, and the map's.
const MOST_KEYS: usize = 32;

/// The largest key `ui.pass_key_size`'s route takes.
const MOST_KEY_SIZE: usize = 80;

/// A growable list of keys as `ui.pass_bindings` fills it and `ui.pass_set`
/// reads it: its keys, their count and its capacity, then the function that
/// grows it. The push grows it only past its capacity and stops the game
/// when it has no such function (client 26430625, read in the decompiler),
/// so the loader's lists hold [`MOST_KEYS`], more than an action has keys.
#[repr(C)]
struct KeyList {
    keys: *mut u8,
    count: u64,
    capacity: u64,
    grow: usize,
}

/// Room for [`MOST_KEYS`] keys, aligned as the game's lists are.
#[repr(C, align(8))]
struct Keys([u8; MOST_KEYS * MOST_KEY_SIZE]);

impl Keys {
    const fn new() -> Self {
        Self([0; MOST_KEYS * MOST_KEY_SIZE])
    }

    /// An empty list over these keys for the game to fill.
    fn list(&mut self) -> KeyList {
        KeyList {
            keys: self.0.as_mut_ptr(),
            count: 0,
            capacity: MOST_KEYS as u64,
            grow: 0,
        }
    }
}

/// The game's builder of an action's keys: a down and an up key for each of
/// its bindings, appended to `list` (client 26430625, read in the
/// decompiler, `0xd20210`).
type BindingsFn = unsafe extern "system" fn(list: *mut KeyList, action: u8);
/// The game's setter of the list its windows let through, a copy of `list`,
/// on the interface context.
type SetFn = unsafe extern "system" fn(context: usize, list: *const KeyList);
/// The game's own rebuild of the list: the voice chat's keys and the list
/// on while voice chat runs, else the list off.
type RefreshFn = unsafe extern "system" fn();

/// The game's list of keys its modal windows let through, and the map's
/// keys the loader adds to it.
#[derive(Debug)]
pub(crate) struct MapKeys {
    bindings: BindingsFn,
    set: SetFn,
    refresh: RefreshFn,
    /// Where the context holds the list's holder.
    holder: usize,
    /// The holder's byte that turns the list on.
    on: usize,
    /// The holder's list: its keys, then their count.
    keys: usize,
    count: usize,
    /// The size of one key.
    size: usize,
    map: ActionNumber,
    /// Whether the map's keys are in the game's list.
    passing: AtomicBool,
}

impl MapKeys {
    /// The list's functions and fields on this build, or what of its code,
    /// table or reflection is missing.
    pub(super) fn resolve(loader: &Loader) -> Result<Self, Missing> {
        let function = |name: Symbol| super::verify(loader, name);
        let offset = |id: Constant| loader.offset(id).ok_or(Missing::Entry(id.id()));
        let size = offset(pinned::ui::PASS_KEY_SIZE)?;
        if size == 0 || size > MOST_KEY_SIZE {
            return Err(Missing::Room(pinned::ui::PASS_KEY_SIZE.id()));
        }
        let ty = catalogue::type_name(catalogue::ids::GAMEPLAY_ACTION);
        let info = loader
            .reflection
            .type_info(ty)
            .map_err(|_| Missing::Reflected(ty, catalogue::ids::GAMEPLAY_ACTION))?;
        let actions = GameActions::new(info.enum_values().iter().map(|v| (v.name(), v.value())));
        let map = actions
            .number(MAP_ACTION)
            .ok_or(Missing::Reflected(ty, MAP_ACTION))?;
        let bindings = function(pinned::ui::PASS_BINDINGS)?;
        // SAFETY: `code_holds` found the builder at `bindings`, its route
        // having matched its code; the type is its signature from the game's
        // own calls of it in `ui.pass_refresh` on client 26430625.
        let bindings = unsafe { core::mem::transmute::<usize, BindingsFn>(bindings) };
        let set = function(pinned::ui::PASS_SET)?;
        // SAFETY: `code_holds` found the setter at `set`, its route having
        // matched its code; the type is its signature from its one call in
        // `ui.pass_refresh` on client 26430625.
        let set = unsafe { core::mem::transmute::<usize, SetFn>(set) };
        let refresh = function(pinned::ui::PASS_REFRESH)?;
        // SAFETY: `code_holds` found the rebuild at `refresh`, its route
        // having matched its code; the game's own calls of it on client
        // 26430625 pass nothing and use no result.
        let refresh = unsafe { core::mem::transmute::<usize, RefreshFn>(refresh) };
        Ok(Self {
            bindings,
            set,
            refresh,
            holder: offset(pinned::ui::PASS_HOLDER)?,
            on: offset(pinned::ui::PASS_ON)?,
            keys: offset(pinned::ui::PASS_KEYS)?,
            count: offset(pinned::ui::PASS_COUNT)?,
            size,
            map,
            passing: AtomicBool::new(false),
        })
    }

    /// Lets the map's keys through the game's modal windows while `wanted`,
    /// adding them again whenever the game rebuilt its own list; once not
    /// wanted, has the game rebuild its own list without them.
    ///
    /// # Safety
    ///
    /// - Called inside a frame of the game's interface update, on its
    ///   thread, where the game's own list functions run.
    /// - `context` is the frame's non-zero interface context.
    pub(in crate::ui::interface) unsafe fn pass(&self, context: usize, wanted: bool) {
        if !wanted && !self.passing.load(Ordering::Relaxed) {
            return;
        }
        let mut map = Keys::new();
        let len = if wanted {
            // SAFETY: this function's `# Safety`: inside the frame on its
            // thread.
            unsafe { self.map_keys(&mut map) }
        } else {
            0
        };
        if len == 0 {
            if self.passing.swap(false, Ordering::Relaxed) {
                // SAFETY: `refresh` is the game's own rebuild, found in
                // `resolve`; it reads the interface context, live in the frame
                // by this function's `# Safety`.
                unsafe { (self.refresh)() };
            }
            return;
        }
        let ours = &map.0[..len];
        // SAFETY: `context` is the frame's live context by this function's
        // `# Safety`; the holder pointer sits at the routed `ui.pass_holder`.
        let holder = unsafe { memory::read::<usize>(context, self.holder) };
        if holder == 0 {
            return;
        }
        let mut game = Keys::new();
        // SAFETY: `holder` is the context's non-zero holder, read above in
        // this frame on its thread.
        if unsafe { self.game_keys(holder, &mut game) }.is_some_and(|keys| ends_with(keys, ours)) {
            self.passing.store(true, Ordering::Relaxed);
            return;
        }
        // SAFETY: `refresh` is the game's own rebuild, found in `resolve`; it
        // reads the interface context, live in the frame by this function's
        // `# Safety`.
        unsafe { (self.refresh)() };
        self.passing.store(false, Ordering::Relaxed);
        // SAFETY: `holder` is the context's non-zero holder, read in this
        // frame on its thread; the rebuild rewrites its list, not the holder.
        let Some(base) = (unsafe { self.game_keys(holder, &mut game) }) else {
            crate::log_once!(
                WARN,
                "interface: the game's list of keys its windows let through does not read; the map's stay out of it"
            );
            return;
        };
        let mut joined = Keys::new();
        let Some(total) = join(base, ours, &mut joined.0[..MOST_KEYS * self.size]) else {
            crate::log_once!(
                WARN,
                "interface: the game's {} keys its windows let through leave no room for the map's",
                base.len() / self.size
            );
            return;
        };
        let mut list = joined.list();
        list.count = (total / self.size) as u64;
        // SAFETY: `set` is the game's setter, found in `resolve`, given the
        // frame's live context and a list of whole keys it copies before it
        // returns.
        unsafe { (self.set)(context, &raw const list) };
        // SAFETY: `holder` is the context's holder, read in this frame; its
        // on byte sits at the routed `ui.pass_on`, which the game's own
        // rebuild writes the same way.
        unsafe { memory::write::<u8>(holder, self.on, 1) };
        self.passing.store(true, Ordering::Relaxed);
    }

    /// Fills `keys` with the map's bound keys; their length in bytes.
    ///
    /// # Safety
    ///
    /// - As for [`MapKeys::pass`]: the builder reads the interface context.
    unsafe fn map_keys(&self, keys: &mut Keys) -> usize {
        let mut list = keys.list();
        // SAFETY: `bindings` is the game's builder, found in `resolve`, called
        // in the frame by the caller's contract with a list of capacity
        // `MOST_KEYS`, more than the eight keys an action's four bindings
        // give, so it never grows it; `map` is a number this build reflects.
        unsafe { (self.bindings)(&raw mut list, self.map.get()) };
        usize::try_from(list.count)
            .ok()
            .filter(|&count| count <= MOST_KEYS)
            .map_or(0, |count| count * self.size)
    }

    /// The keys the game's list holds while it is on, copied into `room`;
    /// none while it is off, `None` for more than fit.
    ///
    /// # Safety
    ///
    /// - `holder` is the interface context's holder, read in this frame on
    ///   the interface thread.
    unsafe fn game_keys<'k>(&self, holder: usize, room: &'k mut Keys) -> Option<&'k [u8]> {
        // SAFETY: the caller's contract; the on byte sits at the routed
        // `ui.pass_on` of the holder.
        let on = unsafe { memory::read::<u8>(holder, self.on) };
        // SAFETY: the caller's contract; the keys pointer sits at the routed
        // `ui.pass_keys` of the holder.
        let keys = unsafe { memory::read::<usize>(holder, self.keys) };
        // SAFETY: the caller's contract; their count sits at the routed
        // `ui.pass_count` of the holder.
        let count = unsafe { memory::read::<u64>(holder, self.count) };
        if on == 0 || count == 0 {
            return Some(&[]);
        }
        let len = usize::try_from(count)
            .ok()
            .filter(|&count| count <= MOST_KEYS && keys != 0)?
            * self.size;
        let out = &mut room.0[..len];
        // SAFETY: `keys` is the game's non-zero list of `count` keys of
        // `size` bytes each, read from the holder in this frame on its thread.
        unsafe { memory::read_bytes(keys, 0, out) };
        Some(out)
    }
}

/// Whether the keys `keys` end with `ours`.
fn ends_with(keys: &[u8], ours: &[u8]) -> bool {
    !ours.is_empty() && keys.ends_with(ours)
}

/// `base`, then `ours`, in `out`; their length, `None` when they do not fit.
fn join(base: &[u8], ours: &[u8], out: &mut [u8]) -> Option<usize> {
    let total = base.len() + ours.len();
    out.get_mut(..total).map(|room| {
        let (first, rest) = room.split_at_mut(base.len());
        first.copy_from_slice(base);
        rest.copy_from_slice(ours);
        total
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_map_s_keys_count_as_passed_only_at_the_end_of_the_game_s_list() {
        let (voice, map) = ([1u8; 4], [2u8; 4]);
        assert!(ends_with(&[voice, map].concat(), &map));
        assert!(ends_with(&map, &map), "voice chat off");
        assert!(!ends_with(&voice, &map), "the game rebuilt its own list");
        assert!(!ends_with(&[map, voice].concat(), &map));
        assert!(!ends_with(&voice, &[]), "an unbound map passes nothing");
    }

    #[test]
    fn the_game_s_keys_stay_first_and_a_list_past_the_room_is_refused() {
        let mut out = [0u8; 6];
        assert_eq!(join(&[1, 1], &[2, 2], &mut out), Some(4));
        assert_eq!(out[..4], [1, 1, 2, 2]);
        assert_eq!(join(&[], &[2, 2], &mut out), Some(2));
        assert_eq!(join(&[1; 5], &[2, 2], &mut out), None);
    }

    #[test]
    fn the_map_action_is_the_generated_gameplay_action_s_name() {
        use enshwick_core::typed::Reflected;
        use enshwick_core::typed::input_mapping::runtime::GameplayAction;
        assert_eq!(
            GameplayAction::UiMap.to_value(),
            enshwick_base::resources::Value::Name(MAP_ACTION.to_owned())
        );
    }

    #[test]
    fn a_list_past_its_capacity_in_keys_is_refused_though_its_bytes_fit() {
        const SIZE: usize = MOST_KEY_SIZE / 2;
        let mut joined = Keys::new();
        let base = [1u8; (MOST_KEYS - 1) * SIZE];
        let ours = [2u8; 2 * SIZE];
        assert!(MOST_KEYS * MOST_KEY_SIZE >= base.len() + ours.len());
        assert_eq!(join(&base, &ours, &mut joined.0[..MOST_KEYS * SIZE]), None);
        assert_eq!(
            join(&base, &ours[..SIZE], &mut joined.0[..MOST_KEYS * SIZE]),
            Some(MOST_KEYS * SIZE)
        );
    }

    #[test]
    fn a_list_holds_more_keys_than_an_action_s_four_bindings_give() {
        const ACTION_KEYS: usize = 2 * 4;
        let mut keys = Keys::new();
        let list = keys.list();
        assert!(
            list.capacity >= 3 * ACTION_KEYS as u64,
            "the voice chat's two and the map"
        );
        assert_eq!((list.count, list.grow), (0, 0));
        assert_eq!(keys.0.len(), MOST_KEYS * MOST_KEY_SIZE);
    }
}
