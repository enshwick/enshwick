//! Skill checks, comfort registrations and experience, each handed to the
//! mods that asked for it.

use core::ffi::c_void;
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick_core::abi::{
    Comfort, ComfortFn, ExperiencePass, ExperiencePassFn, ExperienceRow, Header, HookFlags, Side,
    SkillCheck, SkillCheckFn, Status,
};

use crate::engine::chains::{Next, Onward};
use crate::engine::ecs::Engine;
use crate::engine::fuse::Fuse;
use crate::engine::loader::{Loader, Target};
use crate::engine::loader::{Registered, take};
use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use crate::engine::pinned::{Constant, Symbol};
use enshwick_base::Relock;

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

mod writes;

use writes::{PENDING, WRITERS, WRITES_FUSE};
pub(crate) use writes::{after_step, game_ended, write};

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

// SAFETY: `RegisterFn` is the signature of the table's
// `comfort.base_register`, and this slot is the `next` of the one link `link`
// makes on it, whose detour `register` calls `get`.
static COMFORT_NEXT: Next<RegisterFn> = unsafe { Next::new() };

thread_local! {
    static ROWS: RefCell<Vec<ExperienceRow>> = const { RefCell::new(Vec::new()) };
    static BEFORE: RefCell<Vec<u32>> = const { RefCell::new(Vec::new()) };
}

type RegisterFn = unsafe extern "system" fn(
    system: *mut u8,
    category: u32,
    comfort: i32,
    position: *const f32,
) -> u64;
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
    if levels(loader).is_none() || crate::engine::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    EXPERIENCE_FILTERS
        .relock()
        .push(Registered::new(owner, callback, user));
    Status::OK
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
    let filters = !sealed.experience.is_empty();
    let wants = [
        !sealed.skills.is_empty(),
        !sealed.comfort.is_empty()
            || crate::content::rate_links::wanted(pinned::comfort::BASE_REGISTER),
        filters || writers,
    ];
    let _first = SEALED.set(sealed);
    let _first = LOADED.set(loader);
    FLY_MODE.store(
        symbol(loader, pinned::skill::FLY_MODE_GATE_RETURN).unwrap_or(0),
        Ordering::Relaxed,
    );
    let player_level =
        match crate::engine::ecs::components_match(loader, LEVEL_SYSTEM, &READS, &WRITES) {
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
                    let why = format!("does not match: {why}");
                    let _off = loader.fuse_off(&LEVEL_FUSE, LEVEL_SYSTEM, &why);
                }
                None
            }
        };
    if wants[0]
        && loader.link_fused(
            &SKILLS_FUSE,
            Target::Pinned(pinned::skill::NODE_UNLOCKED),
            &SKILL_NEXT,
            node_unlocked,
            HookFlags::CALLER_FRAME,
        )
    {
        crate::log!(INFO, "skill checks filtered by mods");
    }
    if wants[1]
        && loader.link_fused(
            &COMFORT_FUSE,
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
        && loader.link_fused(
            &LEVEL_FUSE,
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
static SKILLS_FUSE: Fuse = Fuse::new("skill checks");
/// The switch of the comfort link, which the comfort filters and the
/// comfort rate share.
static COMFORT_FUSE: Fuse = Fuse::new("comfort");
/// The switch of the `player_level` link, which the experience filters and
/// the level writes share; each of them has a switch of its own besides,
/// so a fault in one leaves the other running.
static LEVEL_FUSE: Fuse = Fuse::new("experience");
/// The experience filters' switch.
static EXPERIENCE_FUSE: Fuse = Fuse::new("experience filters");

crate::engine::chains::chain_detour! {
    /// The table's `skill.node_unlocked(node, bits, tree)`, whether a skill
    /// node is unlocked: asked of the mods' skill checks for the nodes they
    /// registered.
    fn node_unlocked(node: u32, bits: *const u64, tree: *const u8) -> u8;
    next SKILL_NEXT;
    |onward| {
        let ours = SEALED
            .get()
            .is_some_and(|s| s.skills.iter().any(|(n, _)| *n == node));
        if !ours {
            return onward.call();
        }
        onward.run(&SKILLS_FUSE, "a skill check", |onward| {
            // SAFETY: inside the `node_unlocked` link, on the thread the
            // game called it on, in the chain whose frame `CALLER_FRAME`
            // took.
            unsafe { override_skill(onward, node) }
        })
    }
}

/// Answers a skill check of `node` a mod asked for, after the rest of the
/// chain `onward`.
///
/// # Safety
///
/// - Called from the `node_unlocked` detour on the game's calling thread,
///   inside the chain whose caller frame the link took.
#[cold]
unsafe fn override_skill(onward: &Onward<u8, impl Fn() -> u8>, node: u32) -> u8 {
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
    let vanilla = onward.call();
    let check = SkillCheck {
        header: Header::of::<SkillCheck>(),
        node: enshwick_core::abi::SkillNode::new(node),
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
        .filter(|(n, r)| *n == node && !crate::engine::loader::is_off(r.owner))
    {
        let user = core::ptr::with_exposed_provenance_mut(r.user);
        match crate::engine::profile::call(
            r.owner,
            crate::engine::profile::Feature::Progress,
            // SAFETY: `callback` is a `SkillCheckFn` the mod registered, in
            // its DLL, which stays loaded (`Registered`); `user` is the
            // pointer it registered with it, and `check` lives on this stack
            // for the call.
            || unsafe { (r.callback)(&raw const check, user) },
        ) {
            SkillCheck::UNLOCKED => return 1,
            SkillCheck::LOCKED => return 0,
            _ => {}
        }
    }
    vanilla
}

/// The loader's link on the table's `comfort.base_register`: a comfort
/// provider's value, filtered by the mods and scaled by the comfort rate.
/// Written by hand, not by `chain_detour!`, since it passes the rest of the
/// chain the value it filtered.
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
            unsafe { enshwick_core::raw::read_at::<[f32; 3]>(position.cast(), 0) }
        };
        sealed
            .comfort
            .iter()
            .filter(|r| !crate::engine::loader::is_off(r.owner))
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
                crate::engine::profile::call(
                    r.owner,
                    crate::engine::profile::Feature::Progress,
                    || unsafe { (r.callback)(&raw const call, user) },
                )
            })
    };
    let comfort = COMFORT_FUSE
        .run("a comfort provider", || {
            let rate = crate::content::rate_links::factor(enshwick_core::abi::RateId::COMFORT);
            let filtered = SEALED.get().map_or(comfort, filtered);
            crate::content::rates::scale_comfort(filtered, rate)
        })
        .unwrap_or(comfort);
    // SAFETY: the game's arguments to `comfort.base_register`, with `comfort`
    // filtered, an `i32` like the game's own, passed on to the rest of the
    // chain, which the slot holds since the chain is wired before the target
    // jumps here.
    unsafe { COMFORT_NEXT.get()(system, category, comfort, position) }
}

crate::engine::chains::chain_detour! {
    /// The entry of the reflected `player_level` system, a system entry
    /// taking its context: the experience filters and the level writes run
    /// before vanilla's pass levels the players up.
    fn player_level_link(ctx: *mut u64) -> ();
    next LEVEL_NEXT;
    |onward| {
        let filtered = SEALED.get().is_some_and(|s| !s.experience.is_empty());
        let writes = PENDING.any.load(Ordering::Acquire);
        if filtered || writes {
            onward.run(&LEVEL_FUSE, "the player level pass", |onward| {
                // SAFETY: `ctx` is the game's own context of `player_level`,
                // on the system's thread before vanilla's pass.
                unsafe { level_pass(ctx, filtered, writes) };
                onward.call();
            });
        } else {
            onward.call();
        }
    }
}

/// Runs the experience filters when `filtered` and the level writes when
/// `writes` on the players `player_level` iterates, each under its own
/// switch, and leaves the iterator where it found it.
///
/// # Safety
///
/// - `ctx` is null or the live context the game passed to `player_level`,
///   used on its thread while the system runs, before vanilla's pass.
unsafe fn level_pass(ctx: *mut u64, filtered: bool, writes: bool) {
    if let (Some(loader), Some(engine)) = (LOADED.get(), crate::engine::ecs::resolved())
        && !ctx.is_null()
        // SAFETY: `ctx` is non-null (the `is_null` condition) and the live
        // context the game passed to `player_level`, used on its thread while
        // it runs.
        && (loader.side == Side::SERVER || unsafe { engine.is_world(ctx, crate::engine::world::server_world()) })
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
            let _done = WRITES_FUSE.run("the level writes", || unsafe {
                writes::apply_writes(loader, engine, ctx);
            });
        }
        // SAFETY: `ctx` is the live, non-null query context the game passed
        // to `player_level`, which only this thread writes while the system
        // runs.
        unsafe { engine.set_row(ctx, saved) };
    }
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
                .filter(|r| !crate::engine::loader::is_off(r.owner))
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
                crate::engine::profile::call(
                    r.owner,
                    crate::engine::profile::Feature::Progress,
                    || unsafe {
                        (r.callback)(&raw mut pass, user);
                    },
                );
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
                    .find(|(r, _)| r.entity == enshwick_core::Entity(entity))
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
        entity: enshwick_core::Entity(unsafe { engine.entity(ctx) }),
        level,
        progress,
        need,
        gained,
    })
}
