//! Controls mods declare: the client reads their keys once a frame while
//! its player is in the world with no menu, chat or inventory open, and the
//! server fires those that follow a game action from each player's input.
//!
//! Keys are read beside the game's input, never taken from it: a mod key
//! that is also a game key does both. The game's screen gate is the HUD
//! screen byte of the interface's play data (`ui.hud_screen`), compared
//! with the reflected screens of [`OPEN_SCREENS`]; a build without
//! `InGame` turns the keys off.
//!
//! The server reads its players' input in the inventory system, on
//! whatever thread the game runs it, and fires after the step on its game
//! thread, where a handler may change the world.

use core::cell::RefCell;
use core::ffi::c_void;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use arc_swap::ArcSwapOption;
use enshwick::Entity;
use enshwick::abi::{ControlEvent, ControlFn, Header, PlayerInfo, Side, Status, Str};
use enshwick::controls::{GameAction, Key, Tracker, is_valid_name, phases, valid_phases};
use enshwick_gamedata::catalogue;

use crate::bindings;
use crate::fuse::Fuse;
use crate::host::Loader;
use crate::linked_controls::{Follow, Players};
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::registry::Registry;
use crate::remote::{Carried, Heard, MAX_CARRIED, MAX_CARRIED_TEXT, Message, cut};

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
    /// The mod's action a firing on a client runs on the server; empty for
    /// none.
    action: String,
    fires: u32,
    hold_ms: u32,
    callback: ControlFn,
    user: usize,
}

impl Declared {
    /// The id of the mod that declared it.
    fn mod_id(&self) -> &str {
        let end = self.id.len().saturating_sub(self.name.len() + 1);
        self.id.get(..end).unwrap_or_default()
    }
}

static DECLARED: Registry<Declared> = Registry::new();
static STATE: State = State {
    keys: ArcSwapOption::const_empty(),
    gate: OnceLock::new(),
    linked: OnceLock::new(),
    start: OnceLock::new(),
    kept: Mutex::new(None),
    players: Mutex::new(Players::new()),
};
/// The keys turn off at their first panic, the linked controls at theirs.
static KEYS_FUSE: Fuse = Fuse::new("mod control keys");
static LINKED_FUSE: Fuse = Fuse::new("controls that follow game actions");

/// The controls' state once they are sealed.
struct State {
    /// The keys in force and the controls the server carried; set on the
    /// client at link, by the controls page and by the server's messages.
    keys: ArcSwapOption<Bound>,
    /// Where the client's screen gate reads, once the build has it.
    gate: OnceLock<Gate>,
    /// The controls that follow game actions, once reflection names them.
    linked: OnceLock<Linked>,
    /// The clock both sides' trackers count milliseconds on.
    start: OnceLock<Instant>,
    /// The file's entries for controls no running mod declares, as written,
    /// which a write keeps; `None` while the file does not read, so nothing
    /// writes over it.
    kept: Mutex<Option<toml::Table>>,
    /// The server's players' trackers and the firings that wait for the
    /// step's end.
    players: Mutex<Players>,
}

/// What the client's key reader reads, swapped whole.
#[derive(Debug)]
struct Bound {
    /// The keys in force: the declared controls' by sealed index, then the
    /// carried ones' in their order.
    keys: Vec<[u32; 2]>,
    /// The controls of the server's mods of side server, which this client
    /// runs none of.
    carried: Vec<Heard>,
    /// One more each time `carried` changes, so the reader starts their
    /// trackers over.
    carried_at: u64,
}

/// The HUD screens on which the player is in the world with no window
/// open: those the game's own HUD test counts as no menu (client 520), less
/// the read-only loot chest and the summoner menu, which are windows.
pub(crate) const OPEN_SCREENS: [&str; 6] = [
    "BlueprintMultiTool",
    "OvergrowthMultiTool",
    "DecayMultiTool",
    "ScrewdriverMultiToolBuildingMode",
    "TakeScreenshot",
    "NoUi",
];

/// Where the HUD screen lies from the interface context: the context's
/// G38 state, the play data in the play state, the screen byte in it, and
/// a bit for each screen value with no window open.
#[derive(Debug)]
struct Gate {
    context_state: usize,
    state_play_data: usize,
    hud_screen: usize,
    open: u64,
}

/// The controls that follow game actions: where the digital input sits in
/// `PlayerInput`, and each such control.
#[derive(Debug)]
struct Linked {
    digital: usize,
    follows: Vec<Follow>,
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
    let action_ok = spec.action.is_empty() || is_valid_name(spec.action);
    if !well_named || !keys_ok || !link_ok || !action_ok || !valid_phases(spec.fires) {
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
            action: spec.action.to_owned(),
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
    pub(crate) action: &'a str,
    pub(crate) keys: [u32; 2],
    pub(crate) fires: u32,
    pub(crate) hold_ms: u32,
}

pub(crate) fn link(loader: &Loader) {
    let declared = DECLARED.seal();
    if !declared.is_empty() {
        link_actions(loader, declared);
        too_many_carried(loader, declared);
    }
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
    let table = read.and_then(|text| text.parse::<toml::Table>().ok());
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
            "{}: no running mod declares {}; left alone, or for a server's mod",
            bindings::FILE,
            unknown.join(", ")
        );
    }
    *STATE.kept.relock() = table.map(|table| {
        table
            .into_iter()
            .filter(|(id, _)| !declared.iter().any(|d| d.id == *id))
            .collect()
    });
    let keys: Vec<[u32; 2]> = declared
        .iter()
        .map(|d| {
            in_force(
                d.defaults,
                overrides.get(&d.id).copied().unwrap_or_default(),
            )
        })
        .collect();
    STATE.keys.store(Some(Arc::new(Bound {
        keys,
        carried: Vec::new(),
        carried_at: 0,
    })));
    match gate(loader) {
        Ok(found) => {
            let _first = STATE.gate.set(found);
            crate::log!(INFO, "controls: keys of {} controls on", declared.len());
        }
        Err(why) if declared.is_empty() => {
            crate::log!(INFO, "controls: a server's controls stay off here: {why}");
        }
        Err(why) => {
            loader.feature_off("mod control keys", why.id(), &why.to_string());
        }
    }
}

/// Warns once for each mod of side server with more controls that run an
/// action than one message carries to a client.
fn too_many_carried(loader: &Loader, declared: &[Declared]) {
    for (index, record) in loader.mods.iter().enumerate() {
        let carried = declared
            .iter()
            .filter(|d| d.owner.get() == index && !d.action.is_empty())
            .count();
        if record.manifest.side == Side::SERVER && carried > MAX_CARRIED {
            crate::log!(
                WARN,
                "controls: {} has {carried} controls that run an action; clients get the first {MAX_CARRIED}",
                record.id
            );
        }
    }
}

/// A control's keys: the player's override slot by slot over the mod's.
fn in_force(defaults: [u32; 2], set: bindings::Override) -> [u32; 2] {
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
        .ok_or(ReadGateError::Reflection(
            catalogue::ids::HUD_SCREEN_IN_GAME,
        ))?;
    let screens = catalogue::type_name(catalogue::ids::HUD_SCREEN_TYPE);
    let open = OPEN_SCREENS
        .iter()
        .filter_map(|name| loader.reflection.enum_value(screens, name).ok())
        .chain([in_game])
        .filter_map(|value| 1u64.checked_shl(u32::try_from(value).ok()?))
        .fold(0, |open, bit| open | bit);
    if open == 0 {
        return Err(ReadGateError::Reflection(
            catalogue::ids::HUD_SCREEN_IN_GAME,
        ));
    }
    Ok(Gate {
        context_state: table(pinned::map::CONTEXT_STATE)?,
        state_play_data: table(pinned::map::STATE_PLAY_DATA)?,
        hud_screen: table(pinned::ui::HUD_SCREEN)?,
        open,
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
    const FEATURE: &str = "controls that follow game actions";
    if !declared.iter().any(|d| !d.link.is_empty()) {
        return;
    }
    if !crate::interactions::reads_inputs() {
        loader.feature_off(
            FEATURE,
            crate::interactions::ACTIONS_SYSTEM,
            "the link to the system that reads the players' input did not link",
        );
        return;
    }
    if !crate::events::stepped() {
        loader.feature_off(
            FEATURE,
            pinned::server::POST_STEP.id(),
            "the link after the server's step, where they fire, did not link",
        );
        return;
    }
    let input_type = catalogue::type_name(catalogue::ids::PLAYER_INPUT_TYPE);
    let digital = catalogue::constant(catalogue::ids::PLAYER_INPUT_DIGITAL, &loader.reflection)
        .and_then(|v| usize::try_from(v).ok());
    let Some(digital) = digital else {
        loader.feature_off(
            FEATURE,
            catalogue::ids::PLAYER_INPUT_DIGITAL,
            "the build's reflection lacks it",
        );
        return;
    };
    let mut follows = Vec::new();
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
            follows.push(Follow {
                index,
                bit,
                hold_ms: control.hold_ms,
                fires: control.fires,
            });
        } else {
            crate::log!(
                WARN,
                "control {}: this build has no game action {}; only its keys fire it",
                control.id,
                control.link
            );
        }
    }
    if !follows.is_empty() {
        crate::log!(INFO, "controls: {} follow game actions", follows.len());
        let _first = STATE.linked.set(Linked { digital, follows });
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

/// The client's key reader between frames, on the interface thread.
struct Look {
    /// One tracker per control, declared then carried.
    trackers: Vec<Tracker>,
    /// The slots of each control down at the last look.
    down: Vec<u8>,
    /// The [`Bound::carried_at`] the carried trackers belong to.
    carried_at: u64,
}

thread_local! {
    static LOOK: RefCell<Look> = const {
        RefCell::new(Look {
            trackers: Vec::new(),
            down: Vec::new(),
            carried_at: 0,
        })
    };
}

pub(crate) fn frame(context: usize) {
    let (Some(gate), Some(declared)) = (STATE.gate.get(), DECLARED.sealed()) else {
        return;
    };
    if KEYS_FUSE.is_off() {
        return;
    }
    let Some(bound) = STATE.keys.load_full() else {
        return;
    };
    if bound.keys.is_empty() {
        return;
    }
    let _ran = KEYS_FUSE.run("the mod control keys", || {
        // SAFETY: the one caller passes the frame's live interface context,
        // on the interface thread.
        let open = unsafe { in_game(context, gate) } && !crate::ui::window_shown();
        let window = if open { input::focused_thread() } else { None };
        let now = now_ms();
        let hosting = crate::world::hosting();
        let live = |c: usize| match declared.get(c) {
            Some(d) => fires_here(d, hosting),
            None => !hosting,
        };
        LOOK.with_borrow_mut(|look| {
            if look.carried_at != bound.carried_at {
                look.carried_at = bound.carried_at;
                look.trackers.truncate(declared.len());
                look.down.truncate(declared.len());
            }
            input::keys_down(&bound.keys, window, live, &mut look.down);
            look.trackers.resize(bound.keys.len(), Tracker::default());
            for (index, (tracker, down)) in look.trackers.iter_mut().zip(&look.down).enumerate() {
                if !live(index) {
                    continue;
                }
                if let Some(control) = declared.get(index) {
                    let kind = tracker.look(*down != 0, now, control.hold_ms) & control.fires;
                    for bit in phases(kind) {
                        fire(control, bit, None);
                        if !control.action.is_empty() {
                            let _sent = crate::actions::run_on_server(
                                control.mod_id(),
                                &control.action,
                                "",
                                None,
                            );
                        }
                    }
                } else if let Some(heard) = bound.carried.get(index - declared.len()) {
                    let c = &heard.control;
                    let kind = tracker.look(*down != 0, now, c.hold_ms) & c.fires;
                    for _ in phases(kind) {
                        let _sent =
                            crate::actions::run_on_server(&heard.mod_id, &c.action, "", None);
                    }
                }
            }
        });
    });
}

/// Whether `control`'s keys fire on this client: its mod runs, and a mod of
/// side server only while this client hosts the world, since in another's
/// its server's own copy carries the control.
fn fires_here(control: &Declared, hosting: bool) -> bool {
    crate::host::loader()
        .and_then(|loader| loader.mods.get(control.owner.get()))
        .is_some_and(|m| {
            !m.off.load(core::sync::atomic::Ordering::Acquire)
                && (hosting || m.manifest.side != Side::SERVER)
        })
}

/// Whether `control`'s mod is off.
fn control_off(control: &Declared) -> bool {
    crate::host::loader()
        .and_then(|loader| loader.mods.get(control.owner.get()))
        .is_none_or(|m| m.off.load(core::sync::atomic::Ordering::Acquire))
}

/// Whether the interface's current HUD screen is the world with no window
/// open.
///
/// # Safety
///
/// - `context` is the live interface context of the frame being drawn, on
///   the interface thread.
unsafe fn in_game(context: usize, gate: &Gate) -> bool {
    // SAFETY: the caller passes the live interface context on the interface
    // thread; the offsets are the table's `map.context_state` and
    // `map.state_play_data`.
    let data = unsafe { crate::ui::play_data(context, gate.context_state, gate.state_play_data) };
    if data == 0 {
        return false;
    }
    // SAFETY: `data` is the non-null play data (checked before it), whose
    // HUD screen byte sits at `ui.hud_screen`, as `ui.hud_visible` reads it
    // (clients 410, 426 and 520).
    let screen = unsafe { crate::memory::read::<u8>(data, gate.hud_screen) };
    1u64.checked_shl(screen.into())
        .is_some_and(|bit| gate.open & bit != 0)
}

/// Whether any declared control follows a game action on this build.
pub(crate) fn linked() -> bool {
    STATE.linked.get().is_some() && !LINKED_FUSE.is_off()
}

/// The server's look at `players`, the character entities in its world,
/// once a step during it, on whatever thread runs the inventory system:
/// each linked control looks at its bit of the player's `PlayerInput`,
/// which `input` finds, and what fires waits for [`after_step`].
///
/// The step never waits for the lock: while [`after_step`] holds it, the
/// look waits for the next step, and the trackers lose nothing.
///
/// # Safety
///
/// - `input` returns `None`, null or the player's live `PlayerInput`
///   component, which stays readable and unwritten during the call.
pub(crate) unsafe fn step(players: &[Entity], input: impl Fn(Entity) -> Option<*const u8>) {
    let Some(linked) = STATE.linked.get() else {
        return;
    };
    if LINKED_FUSE.is_off() {
        return;
    }
    let now = now_ms();
    let _ran = LINKED_FUSE.run("the controls that follow game actions", || {
        let Some(mut tracked) = STATE.players.try_relock() else {
            return;
        };
        tracked.step(&linked.follows, players, now, |entity| {
            let input = input(entity).filter(|input| !input.is_null())?;
            // SAFETY: the caller's contract: `input` is the player's live
            // `PlayerInput`, whose digital input word sits at the
            // reflected `fromClient.digitalInput`.
            Some(unsafe { enshwick::raw::read_at::<u64>(input, linked.digital) })
        });
    });
}

/// Fires what the steps' looks queued, on the server's game thread after
/// its step, where a handler may change the world. A firing for a player
/// not in game is dropped, and its control looks again from the start.
pub(crate) fn after_step(loader: &Loader) {
    let (Some(_), Some(declared)) = (STATE.linked.get(), DECLARED.sealed()) else {
        return;
    };
    if !STATE.players.relock().waiting() {
        return;
    }
    let found = crate::world::players(loader).unwrap_or_default();
    let ready = STATE
        .players
        .relock()
        .settle(|entity| found.iter().find(|p| p.entity == entity).copied());
    let _ran = LINKED_FUSE.run("the controls that follow game actions", || {
        for (firing, player) in &ready {
            let Some(control) = declared.get(firing.index) else {
                continue;
            };
            if control_off(control) {
                continue;
            }
            for bit in phases(firing.kind) {
                fire(control, bit, Some(player));
            }
        }
    });
}

/// Forgets the players' trackers and waiting firings, as when the server's
/// world goes away.
pub(crate) fn forget() {
    STATE.players.relock().forget();
}

/// A control as the controls page shows it.
#[derive(Clone, Debug)]
pub(crate) struct Row {
    /// The control's index among the declared, then the carried.
    pub(crate) index: usize,
    /// Its mod's id, which groups the page.
    pub(crate) mod_id: String,
    /// Its mod's name, which heads the group.
    pub(crate) mod_name: String,
    pub(crate) label: String,
    pub(crate) keys: [u32; 2],
}

/// Every control with its keys in force: the declared in declaration
/// order, then the server's carried ones; a declared control the server
/// carries too shows once, as the server's.
pub(crate) fn rows() -> Vec<Row> {
    let (Some(declared), Some(bound), Some(loader)) = (
        DECLARED.sealed(),
        STATE.keys.load_full(),
        crate::host::loader(),
    ) else {
        return Vec::new();
    };
    let carried_ids: Vec<String> = bound.carried.iter().map(Heard::id).collect();
    let own = declared
        .iter()
        .zip(&bound.keys)
        .enumerate()
        .filter(|(_, (d, _))| !carried_ids.contains(&d.id))
        .map(|(index, (d, k))| Row {
            index,
            mod_id: d.mod_id().to_owned(),
            mod_name: loader
                .mods
                .get(d.owner.get())
                .map(crate::host::ModRecord::label)
                .unwrap_or_default(),
            label: d.label.clone(),
            keys: *k,
        });
    let carried = bound
        .carried
        .iter()
        .zip(bound.keys.iter().skip(declared.len()))
        .enumerate()
        .map(|(at, (heard, k))| Row {
            index: declared.len() + at,
            mod_id: heard.mod_id.clone(),
            mod_name: heard.mod_name.clone(),
            label: heard.control.label.clone(),
            keys: *k,
        });
    own.chain(carried).collect()
}

/// Takes the server's controls of its mod `id`, named `name`, on the
/// client: they replace the mod's earlier ones, with the player's keys from
/// `mod-controls.toml`. A client that hosts the world runs the mod itself and
/// takes none.
pub(crate) fn carry(id: &str, name: &str, controls: &[Carried]) {
    if crate::world::hosting() {
        return;
    }
    let Some(declared) = DECLARED.sealed() else {
        return;
    };
    let kept = STATE.kept.relock().clone().unwrap_or_default();
    let (overrides, _) = bindings::parse(&kept.to_string());
    STATE.keys.rcu(|bound| {
        let bound = bound.as_ref()?;
        let carried = crate::remote::carried_after(&bound.carried, id, name, controls);
        Some(Arc::new(with_carried(bound, declared, carried, &overrides)))
    });
    crate::log!(INFO, "controls: the server carries the controls of {id}");
}

fn with_carried(
    bound: &Bound,
    declared: &[Declared],
    carried: Vec<Heard>,
    overrides: &std::collections::BTreeMap<String, bindings::Override>,
) -> Bound {
    let own = bound.keys.get(..declared.len()).unwrap_or_default();
    let mut keys = own.to_vec();
    keys.extend(carried.iter().map(|heard| {
        let id = heard.id();
        // A control this client declares too keeps the keys it has here.
        match declared.iter().position(|d| d.id == id) {
            Some(twin) => own.get(twin).copied().unwrap_or(heard.control.keys),
            None => in_force(
                heard.control.keys,
                overrides.get(&id).copied().unwrap_or_default(),
            ),
        }
    }));
    Bound {
        keys,
        carried,
        carried_at: bound.carried_at.wrapping_add(1),
    }
}

/// Forgets the server's controls, when the client leaves its world.
pub(crate) fn forget_carried() {
    STATE.keys.rcu(|bound| {
        let bound = bound.as_ref()?;
        if bound.carried.is_empty() {
            return Some(Arc::clone(bound));
        }
        let declared = DECLARED.sealed().unwrap_or_default();
        Some(Arc::new(with_carried(
            bound,
            declared,
            Vec::new(),
            &std::collections::BTreeMap::new(),
        )))
    });
}

/// Sends the client of `key`, which just said hello, the controls of every
/// running mod of side server that run an action, in its player's language;
/// false when its link had no room.
pub(crate) fn hello(key: u64) -> bool {
    let (Some(loader), Some(declared)) = (crate::host::loader(), DECLARED.sealed()) else {
        return true;
    };
    if declared.iter().all(|d| d.action.is_empty()) {
        return true;
    }
    crate::texts::for_player(key, || {
        carried_messages(loader, declared)
            .iter()
            .all(|message| crate::net::to_player(key, message))
    })
}

/// One [`Message::Controls`] per running mod of side server with controls
/// that run an action, its texts in the reader's language.
fn carried_messages(loader: &Loader, declared: &[Declared]) -> Vec<Message> {
    let language = crate::texts::language();
    let mut messages = Vec::new();
    for (index, record) in loader.mods.iter().enumerate() {
        if record.manifest.side != Side::SERVER
            || record.off.load(core::sync::atomic::Ordering::Acquire)
            || record.id.len() > MAX_CARRIED_TEXT
        {
            continue;
        }
        let name = record.label();
        let texts = record.texts.relock();
        let controls: Vec<Carried> = declared
            .iter()
            .filter(|d| d.owner.get() == index && !d.action.is_empty())
            .take(MAX_CARRIED)
            .map(|d| {
                let label = texts
                    .as_ref()
                    .and_then(|t| t.get(&d.label, language))
                    .unwrap_or(&d.label);
                Carried {
                    name: d.name.clone(),
                    label: cut(label, MAX_CARRIED_TEXT).to_owned(),
                    action: d.action.clone(),
                    keys: d.defaults,
                    fires: d.fires,
                    hold_ms: d.hold_ms,
                }
            })
            .collect();
        if !controls.is_empty() {
            messages.push(Message::Controls {
                id: record.id.clone(),
                name: cut(&name, MAX_CARRIED_TEXT).to_owned(),
                controls,
            });
        }
    }
    messages
}

/// Whether the client reads mod keys on this build.
pub(crate) fn keys_on() -> bool {
    STATE.gate.get().is_some() && !KEYS_FUSE.is_off()
}

pub(crate) fn set_key(
    loader: &Loader,
    index: usize,
    slot: usize,
    bits: u32,
) -> Result<(), SetKeyError> {
    let (Some(declared), Some(bound)) = (DECLARED.sealed(), STATE.keys.load_full()) else {
        return Ok(());
    };
    let carried = index
        .checked_sub(declared.len())
        .and_then(|at| bound.carried.get(at));
    // A carried control this client declares too sets its own control's key.
    let twin = carried.and_then(|heard| {
        let id = heard.id();
        declared.iter().position(|d| d.id == id)
    });
    let mut keys = bound.keys.clone();
    for at in [Some(index), twin].into_iter().flatten() {
        if let Some(slots) = keys.get_mut(at)
            && let Some(key) = slots.get_mut(slot)
        {
            *key = bits;
        }
    }
    let changed = |defaults: [u32; 2], k: [u32; 2]| {
        let at = |slot: usize| (k[slot] != defaults[slot]).then_some(k[slot]);
        [at(0), at(1)]
    };
    let entries: Vec<(String, bindings::Override)> = declared
        .iter()
        .zip(&keys)
        .map(|(d, k)| (d.id.clone(), changed(d.defaults, *k)))
        .collect();
    let carried = carried
        .filter(|_| twin.is_none())
        .zip(keys.get(index).copied());
    STATE.keys.store(Some(Arc::new(Bound {
        keys,
        carried: bound.carried.clone(),
        carried_at: bound.carried_at,
    })));
    let kept = {
        let mut kept = STATE.kept.relock();
        let Some(kept) = kept.as_mut() else {
            return Err(SetKeyError::Unread);
        };
        if let Some((heard, k)) = carried {
            bindings::set_kept(kept, &heard.id(), changed(heard.control.keys, k));
        }
        kept.clone()
    };
    let path = game_dir(loader).join(bindings::FILE);
    enshwick::files::write_atomically(&path, bindings::render(&entries, &kept))?;
    Ok(())
}

/// Why a key was not saved.
#[derive(Debug, thiserror::Error)]
pub(crate) enum SetKeyError {
    /// The file did not read when the game started; writing would lose it.
    #[error("{} did not read when the game started", bindings::FILE)]
    Unread,
    /// Windows refused the write.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

mod input;

pub(crate) use input::{capture_keys, focused, key_name};
