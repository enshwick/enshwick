//! Controls mods declare: the client reads their keys once a frame while
//! its player is in the world with no menu, chat or inventory open, and the
//! server fires those that follow a game action from each player's input.
//!
//! Keys are read beside the game's input, never taken from it: a mod key
//! that is also a game key does both. The game's screen gate is the HUD
//! screen byte of the interface's play data (`ui.hud_screen`), compared
//! with the reflected `InGame`; a build without it turns the keys off.

use core::cell::RefCell;
use core::ffi::c_void;
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

use arc_swap::ArcSwapOption;
use enshwick::abi::{ControlEvent, ControlFn, Header, PlayerInfo, Side, Status, Str};
use enshwick::controls::{GameAction, Key, Tracker, is_valid_name, phases};
use enshwick_gamedata::catalogue;

use crate::bindings::{self, Override};
use crate::fuse::Fuse;
use crate::host::Loader;
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::registry::Registry;

/// A control a mod declared.
#[derive(Debug)]
pub(crate) struct Declared {
    pub(crate) owner: ModIndex,
    /// `<mod id>:<name>`, as `mod-controls.toml` names it.
    pub(crate) id: String,
    name: String,
    pub(crate) label: String,
    pub(crate) defaults: [u32; 2],
    link: String,
    fires: u32,
    hold_ms: u32,
    callback: ControlFn,
    user: usize,
}

static DECLARED: Registry<Declared> = Registry::new();
static STATE: State = State {
    keys: ArcSwapOption::const_empty(),
    gate: OnceLock::new(),
    linked: OnceLock::new(),
    start: OnceLock::new(),
    kept: Mutex::new(None),
};
/// The keys turn off at their first panic, the linked controls at theirs.
static KEYS_FUSE: Fuse = Fuse::new("mod control keys");
static LINKED_FUSE: Fuse = Fuse::new("controls that follow game actions");

/// The controls' state once they are sealed.
struct State {
    /// The keys in force, by sealed index; set on the client at link and by
    /// the controls page.
    keys: ArcSwapOption<Vec<[u32; 2]>>,
    /// Where the client's screen gate reads, once the build has it.
    gate: OnceLock<Gate>,
    /// The controls that follow game actions, once reflection names them.
    linked: OnceLock<Linked>,
    /// The clock both sides' trackers count milliseconds on.
    start: OnceLock<Instant>,
    /// The file's entries for controls no running mod declares, which a
    /// write keeps; `None` while the file does not read, so nothing writes
    /// over it.
    kept: Mutex<Option<BTreeMap<String, Override>>>,
}

/// Where the HUD screen lies from the interface context: the context's
/// G38 state, the play data in the play state, the screen byte in it, and
/// the value of `InGame`.
#[derive(Debug)]
struct Gate {
    context_state: usize,
    state_play_data: usize,
    hud_screen: usize,
    in_game: u8,
}

/// The controls that follow game actions: where the digital input sits in
/// `PlayerInput`, and each such control with its input bit.
#[derive(Debug)]
struct Linked {
    digital: usize,
    bits: Vec<(usize, u8)>,
}

fn now_ms() -> u64 {
    let start = STATE.start.get_or_init(Instant::now);
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Declares mod `owner`'s control `spec`, during its init.
pub(crate) fn register(
    owner: ModIndex,
    mod_id: &str,
    spec: &Spec<'_>,
    callback: ControlFn,
    user: *mut c_void,
) -> Status {
    let name = spec.name;
    let well_named = is_valid_name(name);
    let keys_ok = spec
        .keys
        .iter()
        .all(|&bits| bits == 0 || Key::from_bits(bits).is_some());
    let link_ok = spec.link.is_empty() || GameAction::ALL.iter().any(|a| a.name() == spec.link);
    if !well_named || !keys_ok || !link_ok {
        return Status::BAD_ARGS;
    }
    let id = format!("{mod_id}:{name}");
    match DECLARED.edit(|declared| declared.iter().any(|d| d.id == id)) {
        Ok(true) => return Status::REFUSED,
        Ok(false) => {}
        Err(_) => return Status::TOO_LATE,
    }
    DECLARED
        .add(Declared {
            owner,
            id,
            name: name.to_owned(),
            label: spec.label.to_owned(),
            defaults: spec.keys,
            link: spec.link.to_owned(),
            fires: spec.fires,
            hold_ms: spec.hold_ms,
            callback,
            user: user.expose_provenance(),
        })
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

/// A control as its mod declares it, its strings read.
pub(crate) struct Spec<'a> {
    pub(crate) name: &'a str,
    pub(crate) label: &'a str,
    pub(crate) link: &'a str,
    pub(crate) keys: [u32; 2],
    pub(crate) fires: u32,
    pub(crate) hold_ms: u32,
}

/// Seals the controls; on the client reads the player's keys and the screen
/// gate, on every side the game actions linked controls follow.
pub(crate) fn link(loader: &Loader) {
    let declared = DECLARED.seal();
    if declared.is_empty() {
        return;
    }
    link_actions(loader, declared);
    if loader.side != Side::CLIENT {
        return;
    }
    let path = game_dir(loader).join(bindings::FILE);
    let read = match std::fs::read_to_string(&path) {
        Ok(text) => Some(text),
        Err(why) if why.kind() == std::io::ErrorKind::NotFound => Some(String::new()),
        Err(why) => {
            crate::log!(WARN, "{}: {why}; the mods' keys apply", bindings::FILE);
            None
        }
    };
    let text = read.clone().unwrap_or_default();
    let (overrides, problems) = bindings::parse(&text);
    let readable = read.is_some() && text.parse::<toml::Table>().is_ok();
    for problem in &problems {
        crate::log!(WARN, "{}: {problem}; the mod's key applies", bindings::FILE);
    }
    let unknown: Vec<&str> = overrides
        .keys()
        .filter(|id| !declared.iter().any(|d| d.id == **id))
        .map(String::as_str)
        .collect();
    if !unknown.is_empty() {
        crate::log!(
            INFO,
            "{}: no running mod declares {}; left alone",
            bindings::FILE,
            unknown.join(", ")
        );
    }
    let kept: BTreeMap<String, Override> = overrides
        .iter()
        .filter(|(id, _)| !declared.iter().any(|d| d.id == **id))
        .map(|(id, set)| (id.clone(), *set))
        .collect();
    *STATE.kept.relock() = readable.then_some(kept);
    let keys: Vec<[u32; 2]> = declared
        .iter()
        .map(|d| {
            in_force(
                d.defaults,
                overrides.get(&d.id).copied().unwrap_or_default(),
            )
        })
        .collect();
    STATE.keys.store(Some(std::sync::Arc::new(keys)));
    match gate(loader) {
        Ok(found) => {
            let _first = STATE.gate.set(found);
            crate::log!(INFO, "controls: keys of {} controls on", declared.len());
        }
        Err(why) => {
            loader.feature_off("mod control keys", why.id(), &why.to_string());
        }
    }
}

/// A control's keys: the player's override slot by slot over the mod's.
fn in_force(defaults: [u32; 2], set: Override) -> [u32; 2] {
    [set[0].unwrap_or(defaults[0]), set[1].unwrap_or(defaults[1])]
}

fn game_dir(loader: &Loader) -> std::path::PathBuf {
    loader
        .mods_dir
        .parent()
        .map_or_else(|| loader.mods_dir.clone(), std::path::Path::to_path_buf)
}

fn gate(loader: &Loader) -> Result<Gate, ReadGateError> {
    let table = |id: pinned::Constant| loader.offset(id).ok_or(ReadGateError::Table(id.id()));
    let in_game = catalogue::constant(catalogue::ids::HUD_SCREEN_IN_GAME, &loader.reflection)
        .and_then(|v| u8::try_from(v).ok())
        .ok_or(ReadGateError::Reflection(
            catalogue::ids::HUD_SCREEN_IN_GAME,
        ))?;
    Ok(Gate {
        context_state: table(pinned::map::CONTEXT_STATE)?,
        state_play_data: table(pinned::map::STATE_PLAY_DATA)?,
        hud_screen: table(pinned::ui::HUD_SCREEN)?,
        in_game,
    })
}

/// Why the screen gate does not read on this build.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
enum ReadGateError {
    /// The build table lacks the entry.
    #[error("this build's table lacks {0}")]
    Table(&'static str),
    /// The build's reflection lacks the fact.
    #[error("the build's reflection lacks {0}")]
    Reflection(&'static str),
}

impl ReadGateError {
    /// The entry or fact missing.
    const fn id(self) -> &'static str {
        match self {
            Self::Table(id) | Self::Reflection(id) => id,
        }
    }
}

fn link_actions(loader: &Loader, declared: &[Declared]) {
    if !declared.iter().any(|d| !d.link.is_empty()) {
        return;
    }
    let input_type = catalogue::type_name(catalogue::ids::PLAYER_INPUT_TYPE);
    let digital = catalogue::constant(catalogue::ids::PLAYER_INPUT_DIGITAL, &loader.reflection)
        .and_then(|v| usize::try_from(v).ok());
    let Some(digital) = digital else {
        loader.feature_off(
            "controls that follow game actions",
            catalogue::ids::PLAYER_INPUT_DIGITAL,
            "the build's reflection lacks it",
        );
        return;
    };
    let mut bits = Vec::new();
    for (index, control) in declared.iter().enumerate() {
        if control.link.is_empty() {
            continue;
        }
        let bit = loader
            .reflection
            .enum_value(input_type, &control.link)
            .ok()
            .and_then(|bit| u8::try_from(bit).ok())
            .filter(|bit| *bit < 64);
        if let Some(bit) = bit {
            bits.push((index, bit));
        } else {
            crate::log!(
                WARN,
                "control {}: this build has no game action {}; only its keys fire it",
                control.id,
                control.link
            );
        }
    }
    if !bits.is_empty() {
        crate::log!(INFO, "controls: {} follow game actions", bits.len());
        let _first = STATE.linked.set(Linked { digital, bits });
    }
}

/// Calls `control`'s callback with `kind`, as the client's own (`player`
/// `None`) or for `player` on the server.
fn fire(control: &Declared, kind: u32, player: Option<&PlayerInfo>) {
    let event = ControlEvent {
        header: Header::of::<ControlEvent>(),
        name: Str::new(&control.name),
        kind,
        server: u8::from(player.is_some()),
        reserved: [0; 3],
        player: player.copied().unwrap_or_else(bytemuck::Zeroable::zeroed),
    };
    // SAFETY: the callback lives in the mod's DLL, which stays loaded, and
    // gets a live event and the pointer its mod declared it with; the safe
    // layer catches its panics, and an `extern "system"` function aborts
    // rather than unwinds.
    crate::profile::call(
        control.owner,
        crate::profile::Feature::Controls,
        || unsafe {
            (control.callback)(
                &raw const event,
                core::ptr::with_exposed_provenance_mut(control.user),
            );
        },
    );
}

thread_local! {
    /// The client's trackers, one per control, on the interface thread.
    static TRACKERS: RefCell<Vec<Tracker>> = const { RefCell::new(Vec::new()) };
    /// The server's trackers by player entity, then control, on its game
    /// thread.
    static SERVER: RefCell<rustc_hash::FxHashMap<(u32, usize), Tracker>> =
        RefCell::new(rustc_hash::FxHashMap::default());
}

/// Looks at the keys once, in the interface frame of the live interface
/// context `context`, on the interface thread.
pub(crate) fn frame(context: usize) {
    let (Some(gate), Some(declared)) = (STATE.gate.get(), DECLARED.sealed()) else {
        return;
    };
    if KEYS_FUSE.is_off() {
        return;
    }
    let Some(keys) = STATE.keys.load_full() else {
        return;
    };
    let _ran = KEYS_FUSE.run("the mod control keys", || {
        // SAFETY: the one caller passes the frame's live interface context,
        // on the interface thread, whose chain to the play data the table's
        // `map.context_state` and `map.state_play_data` give (clients 410,
        // 426 and 520).
        let open = unsafe { in_game(context, gate) } && focused();
        let states = input::read(&keys);
        let now = now_ms();
        TRACKERS.with_borrow_mut(|trackers| {
            if trackers.len() != declared.len() {
                trackers.resize(declared.len(), Tracker::default());
            }
            for ((control, tracker), bound) in
                declared.iter().zip(trackers.iter_mut()).zip(keys.iter())
            {
                if control_off(control) {
                    continue;
                }
                let (down, tapped) = if open {
                    states.chord(*bound)
                } else {
                    (false, false)
                };
                let kind = tracker.look(down, tapped, now, control.hold_ms) & control.fires;
                for bit in phases(kind) {
                    fire(control, bit, None);
                }
            }
        });
    });
}

/// Whether `control`'s mod is off.
fn control_off(control: &Declared) -> bool {
    crate::host::loader()
        .and_then(|loader| loader.mods.get(control.owner.get()))
        .is_none_or(|m| m.off.load(core::sync::atomic::Ordering::Acquire))
}

/// Whether the interface's current HUD screen is the world with nothing
/// open.
///
/// # Safety
///
/// - `context` is the live interface context of the frame being drawn, on
///   the interface thread.
unsafe fn in_game(context: usize, gate: &Gate) -> bool {
    use crate::memory::{read, read_ptr};
    // SAFETY: the caller passes the live interface context, which holds its
    // G38 state pointer at `map.context_state`.
    let state = unsafe { read_ptr::<u8>(context, gate.context_state) };
    if state.is_null() {
        return false;
    }
    // SAFETY: `state` is the context's non-null G38 state (checked before
    // it), whose first word is the play state pointer or null (clients 410,
    // 426 and 520).
    let play = unsafe { read_ptr::<u8>(state.expose_provenance(), 0) };
    if play.is_null() {
        return false;
    }
    // SAFETY: `play` is the non-null play state (checked before it), which
    // holds its play data pointer at `map.state_play_data`.
    let data = unsafe { read_ptr::<u8>(play.expose_provenance(), gate.state_play_data) };
    if data.is_null() {
        return false;
    }
    // SAFETY: `data` is the non-null play data (checked before it), whose
    // HUD screen byte sits at `ui.hud_screen`, as `ui.hud_visible` reads it
    // (clients 410, 426 and 520).
    unsafe { read::<u8>(data.expose_provenance(), gate.hud_screen) == gate.in_game }
}

/// Whether any declared control follows a game action on this build.
pub(crate) fn linked() -> bool {
    STATE.linked.get().is_some() && !LINKED_FUSE.is_off()
}

/// The server's look at player `entity`'s `PlayerInput`, at `input`, once a
/// step on its game thread: each linked control fires on its bit.
///
/// # Safety
///
/// - `input` is the player's live `PlayerInput` component, read during the
///   step on the game thread.
pub(crate) unsafe fn server_input(entity: u32, input: *const u8) {
    let (Some(linked), Some(declared)) = (STATE.linked.get(), DECLARED.sealed()) else {
        return;
    };
    // SAFETY: the caller passes the live `PlayerInput`, whose digital input
    // word sits at the reflected `fromClient.digitalInput`.
    let digital: u64 = unsafe { enshwick::raw::read_at(input, linked.digital) };
    let now = now_ms();
    let _ran = LINKED_FUSE.run("the controls that follow game actions", || {
        let fired: Vec<(usize, u32)> = SERVER.with_borrow_mut(|trackers| {
            linked
                .bits
                .iter()
                .filter_map(|&(index, bit)| {
                    let control = declared.get(index)?;
                    let down = digital & (1 << bit) != 0;
                    let tracker = trackers.entry((entity, index)).or_default();
                    let kind = tracker.look(down, false, now, control.hold_ms) & control.fires;
                    (kind != 0 && !control_off(control)).then_some((index, kind))
                })
                .collect()
        });
        if fired.is_empty() {
            return;
        }
        let Some(player) = crate::host::loader()
            .and_then(|loader| crate::world::players(loader).ok())
            .and_then(|players| players.into_iter().find(|p| p.entity.0 == entity))
        else {
            return;
        };
        for (index, kind) in fired {
            if let Some(control) = declared.get(index) {
                for bit in phases(kind) {
                    fire(control, bit, Some(&player));
                }
            }
        }
    });
}

/// Drops the trackers of players no longer in `players`, the character
/// entities of the step, on the server's game thread.
pub(crate) fn players_seen(players: &[u32]) {
    SERVER.with_borrow_mut(|trackers| trackers.retain(|(entity, _), _| players.contains(entity)));
}

/// Forgets the players' trackers, as when the server's world goes away.
pub(crate) fn forget() {
    SERVER.with_borrow_mut(rustc_hash::FxHashMap::clear);
}

/// A control as the controls page shows it.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    /// The control's index among the declared.
    pub(crate) index: usize,
    pub(crate) owner: ModIndex,
    pub(crate) label: String,
    pub(crate) keys: [u32; 2],
}

/// Every declared control with its keys in force, in declaration order.
pub(crate) fn rows() -> Vec<Row> {
    let (Some(declared), Some(keys)) = (DECLARED.sealed(), STATE.keys.load_full()) else {
        return Vec::new();
    };
    declared
        .iter()
        .zip(keys.iter())
        .enumerate()
        .map(|(index, (d, k))| Row {
            index,
            owner: d.owner,
            label: d.label.clone(),
            keys: *k,
        })
        .collect()
}

/// Whether the client reads mod keys on this build.
pub(crate) fn keys_on() -> bool {
    STATE.gate.get().is_some() && !KEYS_FUSE.is_off()
}

/// Sets control `index`'s key in `slot` to `bits` (0 unbinds) and writes
/// the player's file, keeping its entries for controls no running mod
/// declares.
///
/// # Errors
///
/// The reason the file could not be written, or that it did not read at
/// start, so writing would lose it; the key holds for this run.
pub(crate) fn set_key(
    loader: &Loader,
    index: usize,
    slot: usize,
    bits: u32,
) -> Result<(), std::io::Error> {
    let (Some(declared), Some(keys)) = (DECLARED.sealed(), STATE.keys.load_full()) else {
        return Ok(());
    };
    let mut keys = keys.as_ref().clone();
    if let Some(slots) = keys.get_mut(index)
        && let Some(key) = slots.get_mut(slot)
    {
        *key = bits;
    }
    let mut entries: Vec<(String, Override)> = declared
        .iter()
        .zip(&keys)
        .map(|(d, k)| {
            let changed = |at: usize| (k[at] != d.defaults[at]).then_some(k[at]);
            (d.id.clone(), [changed(0), changed(1)])
        })
        .collect();
    STATE.keys.store(Some(std::sync::Arc::new(keys)));
    let Some(kept) = STATE.kept.relock().clone() else {
        return Err(std::io::Error::other(format!(
            "{} did not read when the game started; fix or delete it",
            bindings::FILE
        )));
    };
    entries.extend(kept);
    let path = game_dir(loader).join(bindings::FILE);
    let fresh = path.with_extension("toml.new");
    std::fs::write(&fresh, bindings::render(&entries))?;
    std::fs::rename(&fresh, &path)
}

mod input;

pub(crate) use input::{capture_keys, focused, key_name};
