//! Skill checks, comfort registrations and experience, each handed to the
//! mods that asked for it.

use core::ffi::c_void;
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::abi::{
    Comfort, ComfortFn, ExperiencePass, ExperiencePassFn, ExperienceRow, Header, HookFlags,
    PROGRESS_ADD_EXPERIENCE, PROGRESS_OPEN, PROGRESS_SET_EXPERIENCE, PROGRESS_SET_LEVEL, Side,
    SkillCheck, SkillCheckFn, Status,
};

use crate::chains::Next;
use crate::ecs::Engine;
use crate::host::{Loader, Target};
use crate::host::{Registered, take};
use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;
use crate::pinned::{Constant, Symbol};
use crate::registry::Registry;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::comfort::BASE_REGISTER.id(),
    pinned::experience::GAINED.id(),
    pinned::experience::LEVEL_CAP.id(),
    pinned::experience::NEED.id(),
    pinned::experience::NEED_FOR_LEVEL.id(),
    pinned::experience::PROGRESS.id(),
    pinned::skill::FLY_MODE_GATE_RETURN.id(),
    pinned::skill::NODE_UNLOCKED.id(),
];

/// `player_level`'s iterator: context, `CurrentTransform`, then `Experience`,
/// `Level` and `PlayerTracking`, each with its change tracking (two stamps,
/// entity record, type and flags, tracker), then `BalancingRegistry`,
/// `BasicPlayerInfos`, `AttributeRegistry` and the `LevelUpEvent` writer.
/// Found on the 1.0 build 1269579, server and client, and checked against
/// the system's reflected components on every build.
const READS: [&str; 1] = ["CurrentTransform"];
const WRITES: [&str; 3] = ["Experience", "Level", "PlayerTracking"];
const WORDS: usize = 24;
const EXPERIENCE: usize = 2;
const STAMP_A: usize = 3;
const STAMP_B: usize = 4;
const ENTITY: usize = 5;
const KIND: usize = 6;
const TRACKER: usize = 7;
const LEVEL: usize = 8;
/// The `Level` set's change tracking follows it as `Experience`'s follows
/// that set.
const LEVEL_TRACKING: usize = LEVEL - EXPERIENCE;
// The `Level` set's tracking starts right after it and ends before the
// balancing registry.
const _: () =
    assert!(STAMP_A + LEVEL_TRACKING == LEVEL + 1 && TRACKER + LEVEL_TRACKING < BALANCING);
const BALANCING: usize = 20;
const REGISTRY: usize = 22;

/// Level and experience writes that wait for the level step at most.
const MOST_WRITES: usize = 256;

/// A level or experience write a mod asked for.
#[derive(Debug, Clone, Copy)]
struct Write {
    entity: u32,
    what: u32,
    value: u32,
    /// A step's end passed since it was asked: the next end drops it, its
    /// character was in no level step.
    seen: bool,
}

/// The level and experience writes mods asked for, which the level step
/// applies.
#[derive(Debug)]
struct Writes {
    waiting: Mutex<Vec<Write>>,
    /// Whether `waiting` holds any, read first by the level step.
    any: AtomicBool,
    /// Whether the level step is linked.
    linked: AtomicBool,
}

static PENDING: Writes = Writes {
    waiting: Mutex::new(Vec::new()),
    any: AtomicBool::new(false),
    linked: AtomicBool::new(false),
};
/// The mods that asked for level and experience writes.
static WRITERS: Registry<ModIndex> = Registry::new();

#[derive(Debug, Default)]
struct Sealed {
    skills: Box<[(u32, Registered<SkillCheckFn>)]>,
    comfort: Box<[Registered<ComfortFn>]>,
    experience: Box<[Registered<ExperiencePassFn>]>,
}

/// The engine pieces experience needs.
#[derive(Debug, Clone, Copy)]
struct Levels {
    /// Never called with level 0: it would read before its table.
    need_for: NeedForFn,
    level_cap: LevelCapFn,
    gained: u32,
    progress: u32,
    need: u32,
}

static SKILLS: Mutex<Vec<(u32, Registered<SkillCheckFn>)>> = Mutex::new(Vec::new());
static COMFORT: Mutex<Vec<Registered<ComfortFn>>> = Mutex::new(Vec::new());
static EXPERIENCE_FILTERS: Mutex<Vec<Registered<ExperiencePassFn>>> = Mutex::new(Vec::new());
static SEALED: OnceLock<Sealed> = OnceLock::new();
static LOADED: OnceLock<&'static Loader> = OnceLock::new();
/// Where the fly mode gate's call returns to; 0 when the build lacks it.
static FLY_MODE: AtomicUsize = AtomicUsize::new(0);
static LEVELS: OnceLock<Option<Levels>> = OnceLock::new();

// SAFETY: `NodeUnlockedFn` is the signature of the table's
// `skill.node_unlocked`, and this slot is the `next` of the one link `link`
// makes on it, whose detour `node_unlocked` calls `get`.
static SKILL_NEXT: Next<NodeUnlockedFn> = unsafe { Next::new() };
// SAFETY: `RegisterFn` is the signature of the table's
// `comfort.base_register`, and this slot is the `next` of the one link `link`
// makes on it, whose detour `register` calls `get`.
static COMFORT_NEXT: Next<RegisterFn> = unsafe { Next::new() };
// SAFETY: `SystemFn` is the signature of the reflected `player_level` system
// entry, and this slot is the `next` of the one link `link` makes on it,
// whose detour `player_level_link` calls `get`.
static LEVEL_NEXT: Next<SystemFn> = unsafe { Next::new() };

thread_local! {
    static ROWS: RefCell<Vec<ExperienceRow>> = const { RefCell::new(Vec::new()) };
    static BEFORE: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

type NodeUnlockedFn = unsafe extern "system" fn(node: u32, bits: *const u64, tree: *const u8) -> u8;
type RegisterFn = unsafe extern "system" fn(
    system: *mut u8,
    category: u32,
    comfort: i32,
    position: *const f32,
) -> u64;
type SystemFn = unsafe extern "system" fn(ctx: *mut u64);
type NeedForFn = unsafe extern "system" fn(level: u32, balancing: u64) -> u32;
type LevelCapFn = unsafe extern "system" fn(balancing: u64) -> u32;

fn symbol(loader: &Loader, name: Symbol) -> Option<usize> {
    loader
        .address(name)
        .and_then(|address| usize::try_from(address).ok())
}

fn levels(loader: &Loader) -> Option<&'static Levels> {
    LEVELS
        .get_or_init(|| {
            let hash = |name: Constant| u32::try_from(loader.build.constant(name.id())?).ok();
            let (need_for, level_cap) = (
                symbol(loader, pinned::experience::NEED_FOR_LEVEL)?,
                symbol(loader, pinned::experience::LEVEL_CAP)?,
            );
            Some(Levels {
                // SAFETY: the table's `experience.need_for_level` is the
                // address of the game function with the `NeedForFn`
                // signature on this build, in the image, which stays mapped.
                need_for: unsafe { core::mem::transmute::<usize, NeedForFn>(need_for) },
                // SAFETY: the table's `experience.level_cap` is the address of
                // the game function with the `LevelCapFn` signature on this
                // build, in the image, which stays mapped.
                level_cap: unsafe { core::mem::transmute::<usize, LevelCapFn>(level_cap) },
                gained: hash(pinned::experience::GAINED)?,
                progress: hash(pinned::experience::PROGRESS)?,
                need: hash(pinned::experience::NEED)?,
            })
        })
        .as_ref()
}

fn too_late(loader: &Loader) -> bool {
    loader.delivery.get().is_some() || SEALED.get().is_some()
}

pub(crate) fn register_skill(
    loader: &Loader,
    owner: ModIndex,
    node: u32,
    callback: SkillCheckFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if symbol(loader, pinned::skill::NODE_UNLOCKED).is_none() {
        return Status::MISSING;
    }
    SKILLS
        .relock()
        .push((node, Registered::new(owner, callback, user)));
    Status::OK
}

pub(crate) fn register_comfort(
    loader: &Loader,
    owner: ModIndex,
    callback: ComfortFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if symbol(loader, pinned::comfort::BASE_REGISTER).is_none() {
        return Status::MISSING;
    }
    COMFORT
        .relock()
        .push(Registered::new(owner, callback, user));
    Status::OK
}

pub(crate) fn register_experience(
    loader: &Loader,
    owner: ModIndex,
    callback: ExperiencePassFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if levels(loader).is_none() || crate::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    EXPERIENCE_FILTERS
        .relock()
        .push(Registered::new(owner, callback, user));
    Status::OK
}

/// Asks for level and experience writes for mod `owner` during `init`
/// ([`PROGRESS_OPEN`]), or queues one for the next level step.
pub(crate) fn write(
    loader: &Loader,
    owner: ModIndex,
    entity: u32,
    what: u32,
    value: u32,
) -> Result<(), Status> {
    if asks(entity, what)? {
        if levels(loader).is_none() || crate::ecs::engine(loader).is_none() {
            return Err(Status::MISSING);
        }
        return WRITERS.add(owner).map_err(|_| Status::TOO_LATE);
    }
    if !PENDING.linked.load(Ordering::Relaxed) {
        return Err(Status::MISSING);
    }
    if !WRITERS.sealed().is_some_and(|mods| mods.contains(&owner)) {
        return Err(Status::REFUSED);
    }
    crate::world::server()?;
    if entity == 0 || !crate::world::player_entities(loader)?.contains(&entity) {
        return Err(Status::REFUSED);
    }
    queue(Write {
        entity,
        what,
        value,
        seen: false,
    })
}

/// Whether `what` asks for the writes, which only `Entity(0)` does, rather
/// than being one.
fn asks(entity: u32, what: u32) -> Result<bool, Status> {
    match what {
        PROGRESS_OPEN if entity == 0 => Ok(true),
        PROGRESS_SET_LEVEL | PROGRESS_SET_EXPERIENCE | PROGRESS_ADD_EXPERIENCE => Ok(false),
        _ => Err(Status::BAD_ARGS),
    }
}

/// Queues `w` for the level step, while fewer than [`MOST_WRITES`] wait.
fn queue(w: Write) -> Result<(), Status> {
    let mut waiting = PENDING.waiting.relock();
    if waiting.len() >= MOST_WRITES {
        return Err(Status::REFUSED);
    }
    waiting.push(w);
    PENDING.any.store(true, Ordering::Release);
    Ok(())
}

/// Drops the writes whose character was in no level step since a step's
/// end passed, on the game thread after the step.
pub(crate) fn after_step() {
    if !PENDING.any.load(Ordering::Acquire) {
        return;
    }
    let mut waiting = PENDING.waiting.relock();
    waiting.retain(|w| !w.seen);
    for w in waiting.iter_mut() {
        w.seen = true;
    }
    PENDING.any.store(!waiting.is_empty(), Ordering::Release);
}

/// What `write` makes of a character's level and experience, `None` for
/// what it leaves: the level, then the experience inside it, what the
/// level needs and the gain the step applies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Written {
    level: Option<u32>,
    progress: Option<u32>,
    need: Option<u32>,
    gained: Option<u32>,
}

/// One write on a character at `need` with `gained` waiting, with the
/// game's level cap and level table.
fn written(w: Write, need: u32, gained: u32, cap: u32, need_for: impl Fn(u32) -> u32) -> Written {
    match w.what {
        PROGRESS_SET_LEVEL => {
            let level = w.value.clamp(1, cap.max(1));
            Written {
                level: Some(level),
                progress: Some(0),
                need: Some(need_for(level)),
                gained: Some(0),
            }
        }
        PROGRESS_SET_EXPERIENCE => Written {
            progress: Some(w.value.min(need.saturating_sub(1))),
            gained: Some(0),
            ..Written::default()
        },
        _ => Written {
            gained: Some(gained.saturating_add(w.value)),
            ..Written::default()
        },
    }
}

/// Applies the waiting writes to the characters of this level step, after
/// the filters and before the game applies the gains.
///
/// # Safety
///
/// - `ctx` is the live query context the game passed to `player_level`,
///   used on that system's thread while it runs, before vanilla runs.
#[cold]
unsafe fn apply_writes(loader: &Loader, engine: &Engine, ctx: *mut u64) {
    let Some(levels) = levels(loader) else {
        return;
    };
    // The step's batches run on several threads at once; a write waits
    // rarely, and only then does a batch take the lock.
    let mut waiting = PENDING.waiting.relock();
    // SAFETY: `ctx` is the live query context of `player_level`, which only
    // this thread writes while the system runs (the caller's contract).
    unsafe { engine.set_row(ctx, 0) };
    // SAFETY: `ctx` is the live query context of `player_level` on its
    // thread (the caller's contract); `WORDS` is that system's iterator size
    // on the layout `link` checks against its reflected components.
    let mut query = unsafe { engine.query::<WORDS>(ctx) };
    while let Some(words) = query.next() {
        // SAFETY: `ctx` is the live query context of `player_level`, used on
        // its thread while it runs (the caller's contract).
        let entity = unsafe { engine.entity(ctx) };
        if !waiting.iter().any(|w| w.entity == entity) || words[BALANCING] == 0 {
            continue;
        }
        let experience = words[EXPERIENCE] as *mut u8;
        let (registry, balancing) = (words[REGISTRY], words[BALANCING]);
        // SAFETY: `level_cap` is the table's `experience.level_cap`, called
        // with the non-zero balancing registry of a live `player_level` row,
        // on that system's thread while it runs.
        let cap = unsafe { (levels.level_cap)(balancing) };
        // SAFETY: `need_for` is the table's `experience.need_for_level`,
        // called by `written` with a level of 1 or more and the balancing
        // registry of a live `player_level` row, on that system's thread.
        let need_for = |level: u32| unsafe { (levels.need_for)(level, balancing) };
        let mut level_written = false;
        waiting.retain(|&w| {
            if w.entity != entity {
                return true;
            }
            // SAFETY: `experience` is the current row's `Experience` set,
            // null or live, and `registry` the same row's attribute registry,
            // on the system's thread.
            let read = |attribute| unsafe { engine.attribute(registry, experience, attribute) };
            let (Some(need), Some(gained)) = (read(levels.need), read(levels.gained)) else {
                return false;
            };
            let change = written(w, need, gained, cap, need_for);
            for (value, attribute) in [
                (change.progress, levels.progress),
                (change.need, levels.need),
                (change.gained, levels.gained),
            ] {
                if let Some(value) = value {
                    // SAFETY: `experience` is the current row's `Experience`
                    // set, which `player_level` writes and nothing else
                    // writes while it runs on this thread; `registry` is the
                    // same row's attribute registry.
                    let _written =
                        unsafe { engine.set_attribute(registry, experience, attribute, value) };
                }
            }
            if let Some(level) = change.level {
                // SAFETY: `words[LEVEL]` is the current row's `Level` set,
                // which `player_level` writes and nothing else writes while
                // it runs on this thread; `registry` is the row's registry.
                level_written |=
                    unsafe { engine.set_level(registry, words[LEVEL] as *mut u8, level) };
            }
            false
        });
        for (shift, wanted) in [(0, true), (LEVEL_TRACKING, level_written)] {
            if wanted {
                // SAFETY: the words are the change tracking of the current
                // row's `Experience` or, `LEVEL_TRACKING` on, `Level` set of
                // the live `player_level` query, which writes both
                // (`WRITES`), on that system's thread while it runs.
                unsafe {
                    engine.mark_changed(
                        words[TRACKER + shift],
                        words[STAMP_A + shift],
                        words[STAMP_B + shift],
                        words[ENTITY + shift],
                        words[KIND + shift],
                    );
                }
            }
        }
    }
    PENDING.any.store(!waiting.is_empty(), Ordering::Release);
}

/// The system that levels players up from their experience.
const LEVEL_SYSTEM: &str = "player_level";

pub(crate) fn link(loader: &'static Loader) {
    let sealed = Sealed {
        skills: take(&SKILLS).into_boxed_slice(),
        comfort: take(&COMFORT).into_boxed_slice(),
        experience: take(&EXPERIENCE_FILTERS).into_boxed_slice(),
    };
    // Sealed whatever else is wanted: a write checks its mod in the seal.
    let writers = !WRITERS.seal().is_empty();
    let wants = [
        !sealed.skills.is_empty(),
        !sealed.comfort.is_empty() || crate::rate_links::wanted(pinned::comfort::BASE_REGISTER),
        !sealed.experience.is_empty() || writers,
    ];
    let _first = SEALED.set(sealed);
    let _first = LOADED.set(loader);
    FLY_MODE.store(
        symbol(loader, pinned::skill::FLY_MODE_GATE_RETURN).unwrap_or(0),
        Ordering::Relaxed,
    );
    let player_level = match crate::ecs::components_match(loader, LEVEL_SYSTEM, &READS, &WRITES) {
        Ok(()) => Some(Target::Found(
            LEVEL_SYSTEM,
            loader
                .reflection
                .system(LEVEL_SYSTEM)
                .ok()
                .map(enshwick_gamedata::reflect::System::entry),
        )),
        Err(why) => {
            if wants[2] {
                crate::log!(WARN, "experience filters off: {why}");
                crate::diag::part_off("experience", &why.to_string());
            }
            None
        }
    };
    if wants[0]
        && loader.link_feature(
            "skill checks",
            Target::Pinned(pinned::skill::NODE_UNLOCKED),
            &SKILL_NEXT,
            node_unlocked,
            HookFlags::CALLER_FRAME,
        )
    {
        crate::log!(INFO, "skill checks filtered by mods");
    }
    if wants[1]
        && loader.link_feature(
            "comfort",
            Target::Pinned(pinned::comfort::BASE_REGISTER),
            &COMFORT_NEXT,
            register,
            HookFlags::NONE,
        )
    {
        crate::log!(INFO, "comfort filtered by mods");
    }
    if wants[2]
        && let Some(player_level) = player_level
        && loader.link_feature(
            "experience",
            player_level,
            &LEVEL_NEXT,
            player_level_link,
            HookFlags::NONE,
        )
    {
        PENDING.linked.store(true, Ordering::Relaxed);
        crate::log!(INFO, "experience filtered or written by mods");
    }
}

/// The skill checks' switch.
static SKILLS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("skill checks");
/// The comfort filters' switch.
static COMFORT_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("comfort filters");
/// The experience filters' switch.
static EXPERIENCE_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("experience filters");

unsafe extern "system" fn node_unlocked(node: u32, bits: *const u64, tree: *const u8) -> u8 {
    let ours = SEALED
        .get()
        .is_some_and(|s| s.skills.iter().any(|(n, _)| *n == node));
    if ours
        // SAFETY: `node`, `bits` and `tree` are the game's arguments to
        // `skill.node_unlocked`, passed on unchanged, on the thread the game
        // called it on, inside the chain whose frame `CALLER_FRAME` took.
        && let Some(answer) = SKILLS_FUSE.run("a skill check", || unsafe {
            override_skill(node, bits, tree)
        })
    {
        return answer;
    }
    // SAFETY: the game's arguments to `skill.node_unlocked`, passed on
    // unchanged to the rest of the chain, which the slot holds since the
    // chain is wired before the target jumps here.
    unsafe { SKILL_NEXT.get()(node, bits, tree) }
}

/// Answers a skill check a mod asked for, after the rest of the chain.
///
/// # Safety
///
/// - `node`, `bits` and `tree` are the game's arguments to
///   `skill.node_unlocked`, unchanged.
/// - Called from the `node_unlocked` detour on the game's calling thread,
///   inside the chain whose caller frame the link took.
#[cold]
unsafe fn override_skill(node: u32, bits: *const u64, tree: *const u8) -> u8 {
    let gate = LOADED.get().map_or(SkillCheck::OTHER, |loader| {
        let frame = (loader.caller_frame)();
        let fly_mode = FLY_MODE.load(Ordering::Relaxed) as u64;
        // SAFETY: the link takes `CALLER_FRAME`, so `frame` was taken at the
        // entry of the running chain (the caller's contract) and its `rsp`,
        // non-zero (checked first), points at the live return address.
        if fly_mode != 0 && frame.rsp != 0 && unsafe { frame.return_address() } == fly_mode {
            SkillCheck::FLY_MODE
        } else {
            SkillCheck::OTHER
        }
    });
    // SAFETY: the game's arguments to `skill.node_unlocked`, unchanged by the
    // caller's contract, passed on to the rest of the chain, which the slot
    // holds since the chain is wired before the target jumps here.
    let vanilla = unsafe { SKILL_NEXT.get()(node, bits, tree) };
    let check = SkillCheck {
        header: Header::of::<SkillCheck>(),
        node: enshwick::abi::SkillNode::new(node),
        gate,
        unlocked: u8::from(vanilla != 0),
        reserved: [0; 3],
    };
    let Some(sealed) = SEALED.get() else {
        return vanilla;
    };
    for (_, r) in sealed
        .skills
        .iter()
        .filter(|(n, r)| *n == node && !crate::host::is_off(r.owner))
    {
        let user = core::ptr::with_exposed_provenance_mut(r.user);
        // SAFETY: `callback` is a `SkillCheckFn` the mod registered, in its
        // DLL, which stays loaded (`Registered`); `user` is the pointer it
        // registered with it, and `check` lives on this stack for the call.
        match crate::profile::call(r.owner, crate::profile::Feature::Progress, || unsafe {
            (r.callback)(&raw const check, user)
        }) {
            SkillCheck::UNLOCKED => return 1,
            SkillCheck::LOCKED => return 0,
            _ => {}
        }
    }
    vanilla
}

unsafe extern "system" fn register(
    system: *mut u8,
    category: u32,
    comfort: i32,
    position: *const f32,
) -> u64 {
    let filtered = |sealed: &Sealed| {
        let at = if position.is_null() {
            [0.0; 3]
        } else {
            // SAFETY: `position` is non-null (the `is_null` branch) and is the
            // game's argument to `comfort.base_register`, the provider's
            // position as three floats, which the game does not write during
            // the call.
            unsafe { enshwick::raw::read_at::<[f32; 3]>(position.cast(), 0) }
        };
        sealed
            .comfort
            .iter()
            .filter(|r| !crate::host::is_off(r.owner))
            .fold(comfort, |value, r| {
                let call = Comfort {
                    header: Header::of::<Comfort>(),
                    category,
                    value,
                    position: at,
                };
                let user = core::ptr::with_exposed_provenance_mut(r.user);
                // SAFETY: `callback` is a `ComfortFn` the mod registered, in
                // its DLL, which stays loaded (`Registered`); `user` is the
                // pointer it registered with it, and `call` lives on this
                // stack for the call.
                crate::profile::call(r.owner, crate::profile::Feature::Progress, || unsafe {
                    (r.callback)(&raw const call, user)
                })
            })
    };
    let comfort = SEALED
        .get()
        .and_then(|sealed| COMFORT_FUSE.run("a comfort provider", || filtered(sealed)))
        .unwrap_or(comfort);
    let comfort = crate::rates::scale_comfort(
        comfort,
        crate::rate_links::factor(enshwick::abi::RateId::COMFORT),
    );
    // SAFETY: the game's arguments to `comfort.base_register`, with `comfort`
    // filtered, an `i32` like the game's own, passed on to the rest of the
    // chain, which the slot holds since the chain is wired before the target
    // jumps here.
    unsafe { COMFORT_NEXT.get()(system, category, comfort, position) }
}

unsafe extern "system" fn player_level_link(ctx: *mut u64) {
    let filtered = SEALED.get().is_some_and(|s| !s.experience.is_empty());
    let writes = PENDING.any.load(Ordering::Acquire);
    if (filtered || writes)
        && let (Some(loader), Some(engine)) = (LOADED.get(), crate::ecs::resolved())
        && !ctx.is_null()
        // SAFETY: `ctx` is non-null (the `is_null` condition) and the live
        // context the game passed to `player_level`, used on its thread while
        // it runs.
        && (loader.side == Side::SERVER || unsafe { engine.is_world(ctx, crate::world::server_world()) })
    {
        // SAFETY: `ctx` is non-null (the `is_null` condition) and the live
        // query context the game passed to `player_level`, used on its thread
        // while it runs.
        let saved = unsafe { engine.row(ctx) };
        if filtered {
            // SAFETY: `ctx` is the live, non-null context the game passed to
            // `player_level`, on its thread while it runs and before vanilla
            // does.
            let _done = EXPERIENCE_FUSE.run("the experience pass", || unsafe {
                experience(loader, engine, ctx);
            });
        }
        if writes {
            // SAFETY: `ctx` is the live, non-null context the game passed to
            // `player_level`, on its thread while it runs and before vanilla
            // does.
            let _done = EXPERIENCE_FUSE.run("the level writes", || unsafe {
                apply_writes(loader, engine, ctx);
            });
        }
        // SAFETY: `ctx` is the live, non-null query context the game passed
        // to `player_level`, which only this thread writes while the system
        // runs.
        unsafe { engine.set_row(ctx, saved) };
    }
    // SAFETY: `ctx` is the game's argument to `player_level`, passed on
    // unchanged to the rest of the chain, which the slot holds since the
    // chain is wired before the target jumps here.
    unsafe { LEVEL_NEXT.get()(ctx) };
}

/// What `need_for` needs during one pass.
struct NeedToken {
    levels: Levels,
    balancing: u64,
}

unsafe extern "system" fn need_for(token: *mut c_void, level: u32) -> u32 {
    if token.is_null() || level == 0 {
        return 0;
    }
    // SAFETY: `token` is non-null (`is_null` returns 0 first) and, by the
    // `ExperiencePass::need_for` contract, the pass's `token`: a `NeedToken`
    // on `experience`'s stack, alive and unwritten while the pass's filters
    // run.
    let token = unsafe { &*token.cast::<NeedToken>() };
    // SAFETY: `need_for` is the table's `experience.need_for_level`, called
    // with a level of 1 or more (`level == 0` returns 0 first) and the
    // balancing registry of a live `player_level` row, on that system's thread
    // while it runs.
    unsafe { (token.levels.need_for)(level, token.balancing) }
}

/// Reads every character, hands them to the filters and writes the gains
/// they changed.
///
/// # Safety
///
/// - `ctx` is the live query context the game passed to `player_level`,
///   used on that system's thread while it runs, before vanilla runs.
unsafe fn experience(loader: &Loader, engine: &Engine, ctx: *mut u64) {
    let (Some(levels), Some(sealed)) = (levels(loader), SEALED.get()) else {
        return;
    };
    ROWS.with_borrow_mut(|rows| {
        BEFORE.with_borrow_mut(|before| {
            rows.clear();
            let mut balancing = 0;
            // SAFETY: `ctx` is the live query context of `player_level`, which
            // only this thread writes while the system runs (the caller's
            // contract).
            unsafe { engine.set_row(ctx, 0) };
            // SAFETY: `ctx` is the live query context of `player_level` on its
            // thread (the caller's contract); `WORDS` is that system's
            // iterator size on the layout `link` checks against its reflected
            // components; `query` drops at the end of this closure.
            let mut query = unsafe { engine.query::<WORDS>(ctx) };
            while let Some(words) = query.next() {
                // SAFETY: `words` is the current row of `query`, the live
                // `player_level` query over `ctx`, on its thread while it runs.
                if let Some(row) = unsafe { read_row(engine, levels, ctx, words) } {
                    balancing = words[BALANCING];
                    rows.push(row);
                }
            }
            if rows.is_empty() || balancing == 0 {
                return;
            }
            before.clear();
            before.extend(rows.iter().map(|r| r.gained));
            let mut token = NeedToken {
                levels: *levels,
                balancing,
            };
            // SAFETY: `level_cap` is the table's `experience.level_cap`, called
            // with the non-zero (`balancing == 0` returns first) balancing
            // registry of a live `player_level` row, on that system's thread
            // while it runs.
            let level_cap = unsafe { (levels.level_cap)(balancing) };
            for r in sealed
                .experience
                .iter()
                .filter(|r| !crate::host::is_off(r.owner))
            {
                let mut pass = ExperiencePass {
                    header: Header::of::<ExperiencePass>(),
                    rows: rows.as_mut_ptr(),
                    row_count: rows.len(),
                    level_cap,
                    need_for: Some(need_for),
                    token: (&raw mut token).cast(),
                };
                let user = core::ptr::with_exposed_provenance_mut(r.user);
                // SAFETY: `callback` is an `ExperiencePassFn` the mod
                // registered, in its DLL, which stays loaded (`Registered`);
                // `user` is the pointer it registered with it; `pass`, the
                // rows and `token` live on this thread for the call.
                crate::profile::call(r.owner, crate::profile::Feature::Progress, || unsafe {
                    (r.callback)(&raw mut pass, user);
                });
            }
            if rows.iter().zip(before.iter()).all(|(r, b)| r.gained == *b) {
                return;
            }
            // SAFETY: `ctx` is the live query context of `player_level`, which
            // only this thread writes while the system runs (the caller's
            // contract).
            unsafe { engine.set_row(ctx, 0) };
            // SAFETY: `ctx` is the live query context of `player_level` on its
            // thread (the caller's contract); `WORDS` is that system's
            // iterator size on the layout `link` checks against its reflected
            // components; `query` drops at the end of this closure.
            let mut query = unsafe { engine.query::<WORDS>(ctx) };
            while let Some(words) = query.next() {
                // SAFETY: `ctx` is the live query context of `player_level`,
                // used on its thread while it runs (the caller's contract).
                let entity = unsafe { engine.entity(ctx) };
                let Some((row, old)) = rows
                    .iter()
                    .zip(before.iter())
                    .find(|(r, _)| r.entity == enshwick::Entity(entity))
                else {
                    continue;
                };
                if row.gained == *old {
                    continue;
                }
                // SAFETY: the words are the change tracking of the current row
                // of the live `player_level` query, which writes `Experience`
                // (`WRITES`), on that system's thread while it runs.
                unsafe {
                    engine.mark_changed(
                        words[TRACKER],
                        words[STAMP_A],
                        words[STAMP_B],
                        words[ENTITY],
                        words[KIND],
                    );
                }
                // SAFETY: `words[EXPERIENCE]` is the current row's
                // `Experience` attribute set, which `player_level` writes and
                // nothing else writes while it runs on this thread;
                // `words[REGISTRY]` is the same row's attribute registry.
                let _written = unsafe {
                    engine.set_attribute(
                        words[REGISTRY],
                        words[EXPERIENCE] as *mut u8,
                        levels.gained,
                        row.gained,
                    )
                };
            }
        });
    });
}

/// One character's experience from the current row, `None` when its
/// attributes do not read.
///
/// # Safety
///
/// - `words` is the current row of the live `player_level` query over
///   `ctx`.
/// - Called on that system's thread while it runs.
unsafe fn read_row(
    engine: &Engine,
    levels: &Levels,
    ctx: *mut u64,
    words: &[u64; WORDS],
) -> Option<ExperienceRow> {
    let experience = words[EXPERIENCE] as *const u8;
    let registry = words[REGISTRY];
    // SAFETY: `words[LEVEL]` is the current row's `Level` attribute set, null
    // or live, and `registry` the same row's attribute registry, on the
    // system's thread (the caller's contract).
    let level = unsafe { engine.level(registry, words[LEVEL] as *const u8) }?;
    // SAFETY: `experience` is the current row's `Experience` attribute set,
    // null or live, and `registry` the same row's attribute registry, on the
    // system's thread (the caller's contract).
    let progress = unsafe { engine.attribute(registry, experience, levels.progress) }?;
    // SAFETY: `experience` is the current row's `Experience` attribute set,
    // null or live, and `registry` the same row's attribute registry, on the
    // system's thread (the caller's contract).
    let need = unsafe { engine.attribute(registry, experience, levels.need) }?;
    // SAFETY: `experience` is the current row's `Experience` attribute set,
    // null or live, and `registry` the same row's attribute registry, on the
    // system's thread (the caller's contract).
    let gained = unsafe { engine.attribute(registry, experience, levels.gained) }?;
    Some(ExperienceRow {
        // SAFETY: `ctx` is the live query context of `player_level`, used on
        // its thread while it runs (the caller's contract).
        entity: enshwick::Entity(unsafe { engine.entity(ctx) }),
        level,
        progress,
        need,
        gained,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued(what: u32, value: u32) -> Write {
        Write {
            entity: 7,
            what,
            value,
            seen: false,
        }
    }

    /// The game's table on the 1.0 builds: 100 a level.
    fn need_for(level: u32) -> u32 {
        level * 100
    }

    #[test]
    fn a_set_level_stays_within_the_cap_and_needs_what_the_game_gives_that_level() {
        let set = |level| written(queued(PROGRESS_SET_LEVEL, level), 900, 40, 25, need_for);
        assert_eq!(
            set(10),
            Written {
                level: Some(10),
                progress: Some(0),
                need: Some(1000),
                gained: Some(0),
            }
        );
        assert_eq!(set(0).level, Some(1), "never below level 1");
        assert_eq!(set(0).need, Some(100));
        assert_eq!(set(99).level, Some(25), "never past the cap");
        assert_eq!(
            written(queued(PROGRESS_SET_LEVEL, 5), 0, 0, 0, need_for).level,
            Some(1),
            "a cap of 0 still leaves level 1"
        );
    }

    #[test]
    fn only_entity_0_asks_for_the_writes_and_another_what_is_refused() {
        assert_eq!(asks(0, PROGRESS_OPEN), Ok(true));
        assert_eq!(asks(7, PROGRESS_OPEN), Err(Status::BAD_ARGS));
        for what in [
            PROGRESS_SET_LEVEL,
            PROGRESS_SET_EXPERIENCE,
            PROGRESS_ADD_EXPERIENCE,
        ] {
            assert_eq!(asks(7, what), Ok(false), "{what}");
        }
        assert_eq!(asks(7, 4), Err(Status::BAD_ARGS));
    }

    #[test]
    fn writes_wait_up_to_the_limit_and_go_after_a_second_step_s_end() {
        for _ in 0..MOST_WRITES {
            assert_eq!(queue(queued(PROGRESS_SET_LEVEL, 3)), Ok(()));
        }
        assert_eq!(queue(queued(PROGRESS_SET_LEVEL, 3)), Err(Status::REFUSED));
        assert!(PENDING.any.load(Ordering::Acquire));
        after_step();
        assert_eq!(PENDING.waiting.relock().len(), MOST_WRITES, "kept one end");
        assert!(PENDING.any.load(Ordering::Acquire));
        assert_eq!(queue(queued(PROGRESS_SET_LEVEL, 4)), Err(Status::REFUSED));
        after_step();
        assert!(PENDING.waiting.relock().is_empty(), "dropped at the second");
        assert!(!PENDING.any.load(Ordering::Acquire));
        assert_eq!(queue(queued(PROGRESS_SET_LEVEL, 5)), Ok(()));
        after_step();
        assert_eq!(PENDING.waiting.relock().len(), 1, "a new one waits again");
    }

    #[test]
    fn set_experience_stays_below_the_need_and_add_experience_adds_to_the_gain() {
        let set = |value, need| {
            written(
                queued(PROGRESS_SET_EXPERIENCE, value),
                need,
                40,
                25,
                need_for,
            )
        };
        assert_eq!(
            set(300, 900),
            Written {
                progress: Some(300),
                gained: Some(0),
                ..Written::default()
            }
        );
        assert_eq!(set(5000, 900).progress, Some(899));
        assert_eq!(set(5, 0).progress, Some(0));
        let add = |value, gained| {
            written(
                queued(PROGRESS_ADD_EXPERIENCE, value),
                900,
                gained,
                25,
                need_for,
            )
        };
        assert_eq!(
            add(500, 40),
            Written {
                gained: Some(540),
                ..Written::default()
            }
        );
        assert_eq!(add(u32::MAX, 40).gained, Some(u32::MAX));
    }
}
