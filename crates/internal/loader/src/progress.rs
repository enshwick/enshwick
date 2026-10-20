//! Skill checks, comfort registrations and experience, each handed to the
//! mods that asked for it.

use core::ffi::c_void;
use std::cell::RefCell;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock, PoisonError};

use crate::chains::Next;
use crate::host::{Registered, take};

use enshwick_loader_api::abi::{
    Comfort, ComfortFn, ExperienceFn, ExperiencePass, ExperienceRow, Header, HookFlags, Side,
    SkillCheck, SkillFn, Status,
};

use crate::ecs::Engine;
use crate::host::Loader;

/// `player_level`'s iterator: context, `CurrentTransform`, then `Experience`,
/// `Level` and `PlayerTracking`, each with its change tracking (two stamps,
/// entity record, type and flags, tracker), then `BalancingRegistry`,
/// `BasicPlayerInfos`, `AttributeRegistry` and the `LevelUpEvent` writer.
const WORDS: usize = 24;
const EXPERIENCE: usize = 2;
const STAMP_A: usize = 3;
const STAMP_B: usize = 4;
const ENTITY: usize = 5;
const KIND: usize = 6;
const TRACKER: usize = 7;
const LEVEL: usize = 8;
const BALANCING: usize = 20;
const REGISTRY: usize = 22;

#[derive(Debug, Default)]
struct Sealed {
    skills: Box<[(u32, Registered<SkillFn>)]>,
    comfort: Box<[Registered<ComfortFn>]>,
    experience: Box<[Registered<ExperienceFn>]>,
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

static SKILLS: Mutex<Vec<(u32, Registered<SkillFn>)>> = Mutex::new(Vec::new());
static COMFORT: Mutex<Vec<Registered<ComfortFn>>> = Mutex::new(Vec::new());
static EXPERIENCE_FILTERS: Mutex<Vec<Registered<ExperienceFn>>> = Mutex::new(Vec::new());
static SEALED: OnceLock<Sealed> = OnceLock::new();
static LOADED: OnceLock<&'static Loader> = OnceLock::new();
/// Where the fly mode gate's call returns to; 0 when the build lacks it.
static FLY_MODE: AtomicUsize = AtomicUsize::new(0);
static LEVELS: OnceLock<Option<Levels>> = OnceLock::new();

// SAFETY: each slot joins the chain of the function its type names.
static SKILL_NEXT: Next<NodeUnlockedFn> = unsafe { Next::new() };
// SAFETY: as above.
static COMFORT_NEXT: Next<RegisterFn> = unsafe { Next::new() };
// SAFETY: as above.
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

fn symbol(loader: &Loader, name: &str) -> Option<usize> {
    loader
        .address(name)
        .and_then(|address| usize::try_from(address).ok())
}

fn levels(loader: &Loader) -> Option<&'static Levels> {
    LEVELS
        .get_or_init(|| {
            let hash = |name: &str| u32::try_from(loader.build.constant(name)?).ok();
            let (need_for, level_cap) = (
                symbol(loader, "experience.need_for_level")?,
                symbol(loader, "experience.level_cap")?,
            );
            Some(Levels {
                // SAFETY: each symbol is the game function with this
                // signature on this build.
                need_for: unsafe { core::mem::transmute::<usize, NeedForFn>(need_for) },
                // SAFETY: as above.
                level_cap: unsafe { core::mem::transmute::<usize, LevelCapFn>(level_cap) },
                gained: hash("experience.gained")?,
                progress: hash("experience.progress")?,
                need: hash("experience.need")?,
            })
        })
        .as_ref()
}

fn too_late(loader: &Loader) -> bool {
    loader.delivery.get().is_some() || SEALED.get().is_some()
}

pub(crate) fn register_skill(
    loader: &Loader,
    owner: usize,
    node: u32,
    callback: SkillFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if symbol(loader, "skill.node_unlocked").is_none() {
        return Status::MISSING;
    }
    SKILLS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push((node, Registered::new(owner, callback, user)));
    Status::OK
}

pub(crate) fn register_comfort(
    loader: &Loader,
    owner: usize,
    callback: ComfortFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if symbol(loader, "comfort.base_register").is_none() {
        return Status::MISSING;
    }
    COMFORT
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Registered::new(owner, callback, user));
    Status::OK
}

pub(crate) fn register_experience(
    loader: &Loader,
    owner: usize,
    callback: ExperienceFn,
    user: *mut c_void,
) -> Status {
    if too_late(loader) {
        return Status::TOO_LATE;
    }
    if levels(loader).is_none() || crate::ecs::engine(loader).is_none() {
        return Status::MISSING;
    }
    EXPERIENCE_FILTERS
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .push(Registered::new(owner, callback, user));
    Status::OK
}

pub(crate) fn link(loader: &'static Loader) {
    let sealed = Sealed {
        skills: take(&SKILLS).into_boxed_slice(),
        comfort: take(&COMFORT).into_boxed_slice(),
        experience: take(&EXPERIENCE_FILTERS).into_boxed_slice(),
    };
    let wants = [
        !sealed.skills.is_empty(),
        !sealed.comfort.is_empty(),
        !sealed.experience.is_empty(),
    ];
    let _first = SEALED.set(sealed);
    let _first = LOADED.set(loader);
    FLY_MODE.store(
        symbol(loader, "skill.fly_mode_gate_return").unwrap_or(0),
        Ordering::Relaxed,
    );
    let player_level = loader
        .reflection
        .system("player_level")
        .map_or(0, enshwick_loader_reflect::System::entry);
    let address = |name| loader.address(name);
    if wants[0]
        && loader.link_feature(
            "skill checks",
            address("skill.node_unlocked"),
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
            address("comfort.base_register"),
            &COMFORT_NEXT,
            register,
            HookFlags::NONE,
        )
    {
        crate::log!(INFO, "comfort filtered by mods");
    }
    if wants[2]
        && loader.link_feature(
            "experience",
            Some(player_level),
            &LEVEL_NEXT,
            player_level_link,
            HookFlags::NONE,
        )
    {
        crate::log!(INFO, "experience filtered by mods");
    }
}

unsafe extern "system" fn node_unlocked(node: u32, bits: *const u64, tree: *const u8) -> u8 {
    let ours = SEALED
        .get()
        .is_some_and(|s| s.skills.iter().any(|(n, _)| *n == node));
    if !ours {
        // SAFETY: the rest of the chain takes the arguments the game passed.
        return unsafe { SKILL_NEXT.get()(node, bits, tree) };
    }
    // SAFETY: as above.
    unsafe { override_skill(node, bits, tree) }
}

/// # Safety
///
/// The game's arguments to `node_unlocked`.
#[cold]
unsafe fn override_skill(node: u32, bits: *const u64, tree: *const u8) -> u8 {
    let gate = LOADED.get().map_or(SkillCheck::OTHER, |loader| {
        let frame = (loader.caller_frame)();
        let fly_mode = FLY_MODE.load(Ordering::Relaxed) as u64;
        // SAFETY: the frame was taken at entry of this chain.
        if fly_mode != 0 && frame.rsp != 0 && unsafe { frame.return_address() } == fly_mode {
            SkillCheck::FLY_MODE
        } else {
            SkillCheck::OTHER
        }
    });
    // SAFETY: the rest of the chain takes the arguments the game passed.
    let vanilla = unsafe { SKILL_NEXT.get()(node, bits, tree) };
    let check = SkillCheck {
        header: Header::of::<SkillCheck>(),
        node,
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
        // SAFETY: the callback lives in the mod's DLL and takes the pointer
        // it registered; the check is valid for the call.
        match unsafe { (r.callback)(&raw const check, user) } {
            1 => return 1,
            2 => return 0,
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
    let comfort = SEALED.get().map_or(comfort, |sealed| {
        let at = if position.is_null() {
            [0.0; 3]
        } else {
            // SAFETY: the game passes the provider's position as three floats.
            unsafe { position.cast::<[f32; 3]>().read_unaligned() }
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
                // SAFETY: the callback lives in the mod's DLL and takes the
                // pointer it registered; the call is valid for it.
                unsafe { (r.callback)(&raw const call, user) }
            })
    });
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { COMFORT_NEXT.get()(system, category, comfort, position) }
}

unsafe extern "system" fn player_level_link(ctx: *mut u64) {
    if let (Some(loader), Some(engine)) = (LOADED.get(), crate::ecs::resolved())
        && !ctx.is_null()
        // SAFETY: the system's live context.
        && (loader.side == Side::SERVER || unsafe { engine.is_world(ctx, crate::world::server_world()) })
    {
        // SAFETY: the system's live context.
        let saved = unsafe { engine.row(ctx) };
        // SAFETY: as above, before vanilla runs.
        let _done = std::panic::catch_unwind(|| unsafe { experience(loader, engine, ctx) });
        // SAFETY: as above.
        unsafe { engine.set_row(ctx, saved) };
    }
    // SAFETY: the original system takes its own context.
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
    // SAFETY: the token of the running pass.
    let token = unsafe { &*token.cast::<NeedToken>() };
    // SAFETY: the game's need per level with the row's balancing registry.
    unsafe { (token.levels.need_for)(level, token.balancing) }
}

/// Reads every character, hands them to the filters and writes the gains
/// they changed.
///
/// # Safety
///
/// `ctx` is the live context of `player_level`, before vanilla runs.
unsafe fn experience(loader: &Loader, engine: &Engine, ctx: *mut u64) {
    let (Some(levels), Some(sealed)) = (levels(loader), SEALED.get()) else {
        return;
    };
    ROWS.with_borrow_mut(|rows| {
        BEFORE.with_borrow_mut(|before| {
            rows.clear();
            let mut balancing = 0;
            // SAFETY: the system's live context.
            unsafe { engine.set_row(ctx, 0) };
            // SAFETY: as above, with the system's iterator size.
            let mut query = unsafe { engine.query::<WORDS>(ctx) };
            while let Some(words) = query.next() {
                // SAFETY: the words of the current row.
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
            // SAFETY: the game's cap lookup with a row's balancing registry.
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
                // SAFETY: the callback lives in the mod's DLL and takes the
                // pointer it registered; the pass is valid for the call.
                unsafe { (r.callback)(&raw mut pass, user) };
            }
            if rows.iter().zip(before.iter()).all(|(r, b)| r.gained == *b) {
                return;
            }
            // SAFETY: the system's live context.
            unsafe { engine.set_row(ctx, 0) };
            // SAFETY: as above, with the system's iterator size.
            let mut query = unsafe { engine.query::<WORDS>(ctx) };
            while let Some(words) = query.next() {
                // SAFETY: the system's live context.
                let entity = unsafe { engine.entity(ctx) };
                let Some((row, old)) = rows
                    .iter()
                    .zip(before.iter())
                    .find(|(r, _)| r.entity == entity)
                else {
                    continue;
                };
                if row.gained == *old {
                    continue;
                }
                // SAFETY: the change-tracking words of the current row, which
                // `player_level` writes.
                unsafe {
                    engine.mark_changed(
                        words[TRACKER],
                        words[STAMP_A],
                        words[STAMP_B],
                        words[ENTITY],
                        words[KIND],
                    );
                }
                // SAFETY: the row's `Experience`, which this system writes.
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

/// # Safety
///
/// `words` are the current row of `player_level`'s live query `ctx`.
unsafe fn read_row(
    engine: &Engine,
    levels: &Levels,
    ctx: *mut u64,
    words: &[u64; WORDS],
) -> Option<ExperienceRow> {
    let experience = words[EXPERIENCE] as *const u8;
    let registry = words[REGISTRY];
    // SAFETY: the row's `Level` and attribute registry.
    let level = unsafe { engine.level(registry, words[LEVEL] as *const u8) }?;
    // SAFETY: the row's `Experience` and attribute registry.
    let progress = unsafe { engine.attribute(registry, experience, levels.progress) }?;
    // SAFETY: as above.
    let need = unsafe { engine.attribute(registry, experience, levels.need) }?;
    // SAFETY: as above.
    let gained = unsafe { engine.attribute(registry, experience, levels.gained) }?;
    Some(ExperienceRow {
        // SAFETY: the system's live context.
        entity: unsafe { engine.entity(ctx) },
        level,
        progress,
        need,
        gained,
    })
}
