//! The knowledge guard: what clients ask for in their input, checked before
//! the game queues it.
//!
//! A client asks for knowledge with a `TriggerKnowledgeAction` in its
//! input; `handle_knowledge_trigger_input` queues it for the player when the
//! action is new and well formed, and checks nothing else. A vanilla client
//! asks only for what its interface marks: a player's bools (hints shown,
//! screens opened) and the seen bit (0) of a player's flags. The guard lets
//! those through, and refuses ids of the world's, counters, other flag bits,
//! ids that give experience or skill points, and quests. Mods' filters run
//! after it with the verdict so far.
//!
//! The guard watches by default: it logs what it would refuse and lets it
//! through, so mods' filters see it allowed, with the reason it would
//! refuse. Enforcing, its refusals stand unless a filter allows them. A
//! filter's refusal stands in both modes.
//!
//! The site is the handler's test of the game's own check, at the server's
//! worker threads: clearing `al` there takes the path of a malformed action,
//! which consumes the request without queueing it.

use crate::registry::Registry;
use core::ffi::c_void;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};

use enshwick::abi::{
    Header, HookFlags, KnowledgeInfo, KnowledgeInput, KnowledgeInputFn, Registers, Status,
};

use crate::chains::LOADER;
use crate::host::{Loader, Registered};
use crate::locks::Relock;
use crate::mid::MidLink;
use crate::settings::KnowledgeGuard;

use super::{Layout, definition, layout, quest_set, read};

/// What the guard does, from the loader's settings: one of the `MODE_`
/// values.
static MODE: AtomicU8 = AtomicU8::new(MODE_WATCH);
const MODE_WATCH: u8 = 0;
const MODE_ENFORCE: u8 = 1;
const MODE_OFF: u8 = 2;
static FILTERS: Registry<Registered<KnowledgeInputFn>> = Registry::new();
static SITE: OnceLock<Site> = OnceLock::new();
/// Requests the guard would refuse but let through, watching.
static WOULD_REFUSE: AtomicU32 = AtomicU32::new(0);
/// Requests refused, by the guard enforcing or by a filter.
static REFUSED: AtomicU32 = AtomicU32::new(0);
/// Lines logged so far; the log stops at [`LOGGED`].
static LINES: AtomicU32 = AtomicU32::new(0);
const LOGGED: u32 = 64;

/// Where the site keeps the request and what the guard reads of a
/// definition.
#[derive(Debug, Clone, Copy)]
struct Site {
    /// The action in the input row `rbx`, a trigger request: its id first,
    /// its definition kind and its value.
    action: usize,
    kind: usize,
    value: usize,
    /// A player definition's experience id and skill points.
    experience: usize,
    skill_points: usize,
}

/// Sets what the guard does, from `knowledge_guard`.
pub(crate) fn configure(mode: KnowledgeGuard) {
    let mode = match mode {
        KnowledgeGuard::Watch => MODE_WATCH,
        KnowledgeGuard::Enforce => MODE_ENFORCE,
        KnowledgeGuard::Off => MODE_OFF,
    };
    MODE.store(mode, Ordering::Release);
}

/// The guard's line for the status summary, once its site is in.
pub(crate) fn status() -> Option<String> {
    SITE.get()?;
    let mode = match MODE.load(Ordering::Acquire) {
        MODE_WATCH => "watching",
        MODE_ENFORCE => "enforcing",
        _ => "mods' filters only",
    };
    Some(format!(
        "{mode}; {} requests it would refuse let through, {} refused",
        WOULD_REFUSE.load(Ordering::Relaxed),
        REFUSED.load(Ordering::Relaxed)
    ))
}

/// Registers mod `owner`'s filter.
pub(crate) fn register(
    loader: &Loader,
    owner: usize,
    callback: KnowledgeInputFn,
    user: *mut c_void,
) -> Status {
    if FILTERS.is_sealed() {
        return Status::TOO_LATE;
    }
    if site(loader).is_none() || loader.address("knowledge.input_check").is_none() {
        return Status::MISSING;
    }
    FILTERS
        .add(Registered::new(owner, callback, user))
        .map_or(Status::TOO_LATE, |()| Status::OK)
}

fn site(loader: &Loader) -> Option<Site> {
    let field = |name: &str| {
        loader
            .reflection
            .field_offset("keen::PlayerKnowledgeObjectResource", name)
            .ok()
            .and_then(|v| usize::try_from(v).ok())
    };
    Some(Site {
        action: loader.offset("knowledge.input_action")?,
        kind: loader.offset("knowledge.request_kind")?,
        value: loader.offset("knowledge.request_value")?,
        experience: field("experienceGainId")?,
        skill_points: field("unlockedSkillPoints")?,
    })
}

/// Seals the filters and adds the site when the guard is on or a mod asked.
pub(crate) fn link(loader: &Loader) {
    let filters = FILTERS.seal();
    let mode = MODE.load(Ordering::Acquire);
    let wanted = mode != MODE_OFF || !filters.is_empty();
    if !wanted {
        return;
    }
    let (Some(site), Some(_), Some(address)) = (
        site(loader),
        layout(loader),
        loader.address("knowledge.input_check"),
    ) else {
        crate::log!(WARN, "knowledge guard off: not in this build");
        return;
    };
    if crate::ecs::engine(loader).is_none() {
        crate::log!(WARN, "knowledge guard off: no entity reads on this build");
        return;
    }
    let _first = SITE.set(site);
    let link = MidLink {
        owner: LOADER,
        handler: check,
        user: 0,
        priority: i32::MIN,
        flags: HookFlags::NONE,
    };
    let status = loader.mids.relock().add(address, link);
    if status.is_ok() {
        crate::log!(
            INFO,
            "knowledge guard on: {}",
            match mode {
                MODE_WATCH => "logging what a vanilla client never asks for, refusing nothing",
                MODE_ENFORCE => "refusing what a vanilla client never asks for",
                _ => "mods' filters only",
            }
        );
    } else {
        crate::log!(WARN, "knowledge guard off: refused ({})", status.0);
    }
}

/// What the guard knows of an id's definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Definition {
    scope: u8,
    kind: u8,
    experience: u32,
    skill_points: u8,
    quest: bool,
}

/// Why the guard refuses a request for `kind` and `value` of an id defined
/// as `definition`, or [`KnowledgeInput::ALLOWED`].
fn refusal(definition: Option<Definition>, kind: u8, value: u32) -> u32 {
    let Some(def) = definition else {
        return KnowledgeInput::UNKNOWN;
    };
    if def.kind != kind {
        KnowledgeInput::KIND
    } else if def.scope != KnowledgeInfo::PLAYER {
        KnowledgeInput::WORLD
    } else if kind == KnowledgeInfo::COUNTER {
        KnowledgeInput::COUNTER
    } else if kind == KnowledgeInfo::FLAGS && value != 0 {
        KnowledgeInput::FLAG_BIT
    } else if def.experience != 0 {
        KnowledgeInput::EXPERIENCE
    } else if def.skill_points != 0 {
        KnowledgeInput::SKILL_POINTS
    } else if def.quest {
        KnowledgeInput::QUEST
    } else if kind == KnowledgeInfo::BOOL || kind == KnowledgeInfo::FLAGS {
        KnowledgeInput::ALLOWED
    } else {
        KnowledgeInput::KIND
    }
}

/// The definition of `id` in the running world, read from any thread: the
/// definitions and the journal stay put while a world runs.
fn define(layout: &Layout, site: Site, id: u32) -> Option<Definition> {
    let server = crate::world::SERVER.load(Ordering::Acquire);
    if server == 0 {
        return None;
    }
    let (game, state) = super::store_of(layout, server).ok()?;
    let (scope, def) = definition(layout, state, id)?;
    let player = scope == KnowledgeInfo::PLAYER;
    let (experience, skill_points) = if player {
        // SAFETY: `def` is a player's definition `definition` found in the
        // live blob, which stays put while the world runs; its experience
        // id sits at the reflected `experienceGainId` field.
        let experience = unsafe { read(def, site.experience) };
        // SAFETY: `def` is a player's definition `definition` found in the
        // live blob, which stays put while the world runs; its skill points
        // sit at the reflected `unlockedSkillPoints` field.
        let skill_points = unsafe { read(def, site.skill_points) };
        (experience, skill_points)
    } else {
        (0, 0)
    };
    Some(Definition {
        scope,
        // SAFETY: `def` is the definition `definition` found in the live
        // blob, which stays put while the world runs; its kind sits at the
        // reflected `type` field.
        kind: unsafe { read(def, layout.def_kind) },
        experience,
        skill_points,
        quest: player && quest_set(layout, game).is_some_and(|set| set.contains(&id)),
    })
}

/// The verdict on `call` after the guard and every live filter.
fn decide(call: &mut KnowledgeInput) -> bool {
    if let Some(filters) = FILTERS.sealed() {
        for f in filters.iter().filter(|f| f.live()) {
            // SAFETY: `f` is live, so the mod's DLL that holds the callback
            // stays loaded; the callback gets the user pointer its mod
            // registered and `call`, which lives for the call.
            let verdict =
                crate::profile::call(f.owner, crate::profile::Feature::Vetoes, || unsafe {
                    (f.callback)(&raw const *call, f.user())
                });
            call.allowed = u32::from(verdict != 0);
        }
    }
    call.allowed != 0
}

/// The guard's own verdict on a request it refuses for `refusal`, in `mode`.
fn guard_allows(mode: u8, refusal: u32) -> bool {
    refusal == KnowledgeInput::ALLOWED || mode != MODE_ENFORCE
}

/// Counts and logs a request the guard would refuse, watching, when
/// `refused` is false, or one refused.
#[cold]
fn note(call: &KnowledgeInput, refused: bool) {
    let (counter, verb) = if refused {
        (&REFUSED, "refused")
    } else {
        (&WOULD_REFUSE, "would refuse, let through")
    };
    let _count = counter.fetch_add(1, Ordering::Relaxed);
    let line = LINES.fetch_add(1, Ordering::Relaxed);
    if line < LOGGED {
        crate::log!(
            INFO,
            "knowledge guard: {verb} id {:#x} (kind {}, value {}, scope {}) asked for by player {}, reason {}",
            call.id,
            call.kind,
            call.value,
            call.scope,
            call.player,
            call.refusal
        );
    } else if line == LOGGED {
        crate::log!(
            INFO,
            "knowledge guard: further requests are not logged; the status summary counts them"
        );
    }
}

/// A register as an address; registers are 64 bits, as addresses are here.
fn address(register: u64) -> usize {
    usize::try_from(register).unwrap_or(0)
}

/// The switch of the knowledge guard.
static GUARD_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("knowledge guard");

unsafe extern "system" fn check(registers: *mut Registers, _user: *mut c_void) {
    let _caught = GUARD_FUSE.run("the check", || {
        // SAFETY: the mid-function stub passes its own saved register block,
        // or null, live and unaliased for the call.
        let Some(registers) = (unsafe { registers.as_mut() }) else {
            return;
        };
        if registers.rax.trailing_zeros() >= 8 {
            return;
        }
        let (Some(&site), Some(loader)) = (SITE.get(), crate::host::loader()) else {
            return;
        };
        let Some(layout) = layout(loader) else {
            return;
        };
        let action = address(registers.rbx).wrapping_add(site.action);
        // SAFETY: at the site `rbx` is the player's input row, which holds
        // the action at the build table's `knowledge.input_action` offset, a
        // trigger request with its u32 id first (1.0 build 1269579).
        let id: u32 = unsafe { read(action, 0) };
        // SAFETY: `action` is the input row's trigger request at the site
        // (1.0 build 1269579); its u8 definition kind sits at the build
        // table's `knowledge.request_kind` offset.
        let kind: u8 = unsafe { read(action, site.kind) };
        // SAFETY: `action` is the input row's trigger request at the site
        // (1.0 build 1269579); its u32 value sits at the build table's
        // `knowledge.request_value` offset.
        let value: u32 = unsafe { read(action, site.value) };
        let player = crate::ecs::resolved().map_or(0, |engine| {
            // SAFETY: at the site `rdi` is the running system's live query
            // context (1.0 build 1269579), and this runs on that system's
            // worker thread.
            unsafe {
                engine.entity(core::ptr::with_exposed_provenance_mut(address(
                    registers.rdi,
                )))
            }
        });
        let definition = define(layout, site, id);
        let refusal = refusal(definition, kind, value);
        let mode = MODE.load(Ordering::Acquire);
        let mut call = KnowledgeInput {
            header: Header::of::<KnowledgeInput>(),
            player,
            id,
            kind,
            scope: definition.map_or(KnowledgeInput::NO_SCOPE, |d| d.scope),
            reserved: 0,
            value,
            refusal,
            allowed: u32::from(guard_allows(mode, refusal)),
        };
        if !decide(&mut call) {
            registers.rax &= !0xff;
            note(&call, true);
        } else if mode == MODE_WATCH && refusal != KnowledgeInput::ALLOWED {
            note(&call, false);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(kind: u8) -> Definition {
        Definition {
            scope: KnowledgeInfo::PLAYER,
            kind,
            experience: 0,
            skill_points: 0,
            quest: false,
        }
    }

    #[test]
    fn a_vanilla_clients_requests_go_through() {
        let bool_id = player(KnowledgeInfo::BOOL);
        let flags = player(KnowledgeInfo::FLAGS);
        assert_eq!(
            refusal(Some(bool_id), KnowledgeInfo::BOOL, 0),
            KnowledgeInput::ALLOWED
        );
        assert_eq!(
            refusal(Some(flags), KnowledgeInfo::FLAGS, 0),
            KnowledgeInput::ALLOWED
        );
    }

    #[test]
    fn what_a_vanilla_client_never_asks_for_is_refused() {
        let world = Definition {
            scope: KnowledgeInfo::WORLD,
            ..player(KnowledgeInfo::BOOL)
        };
        let cases = [
            (None, KnowledgeInfo::BOOL, 0, KnowledgeInput::UNKNOWN),
            (
                Some(player(KnowledgeInfo::BOOL)),
                KnowledgeInfo::FLAGS,
                0,
                KnowledgeInput::KIND,
            ),
            (Some(world), KnowledgeInfo::BOOL, 0, KnowledgeInput::WORLD),
            (
                Some(player(KnowledgeInfo::COUNTER)),
                KnowledgeInfo::COUNTER,
                5,
                KnowledgeInput::COUNTER,
            ),
            (
                Some(player(KnowledgeInfo::FLAGS)),
                KnowledgeInfo::FLAGS,
                1,
                KnowledgeInput::FLAG_BIT,
            ),
            (
                Some(Definition {
                    experience: 7,
                    ..player(KnowledgeInfo::BOOL)
                }),
                KnowledgeInfo::BOOL,
                0,
                KnowledgeInput::EXPERIENCE,
            ),
            (
                Some(Definition {
                    skill_points: 1,
                    ..player(KnowledgeInfo::BOOL)
                }),
                KnowledgeInfo::BOOL,
                0,
                KnowledgeInput::SKILL_POINTS,
            ),
            (
                Some(Definition {
                    quest: true,
                    ..player(KnowledgeInfo::BOOL)
                }),
                KnowledgeInfo::BOOL,
                0,
                KnowledgeInput::QUEST,
            ),
            (Some(player(9)), 9, 0, KnowledgeInput::KIND),
        ];
        for (def, kind, value, want) in cases {
            assert_eq!(refusal(def, kind, value), want, "{def:?} {kind} {value}");
        }
    }

    #[test]
    fn only_an_enforcing_guard_refuses() {
        let reason = KnowledgeInput::QUEST;
        assert!(guard_allows(MODE_WATCH, reason));
        assert!(guard_allows(MODE_OFF, reason));
        assert!(!guard_allows(MODE_ENFORCE, reason));
        assert!(guard_allows(MODE_ENFORCE, KnowledgeInput::ALLOWED));
    }

    #[test]
    fn without_filters_the_guards_verdict_stands() {
        let mut call = KnowledgeInput {
            header: Header::of::<KnowledgeInput>(),
            player: 1,
            id: 2,
            kind: KnowledgeInfo::BOOL,
            scope: KnowledgeInfo::PLAYER,
            reserved: 0,
            value: 0,
            refusal: KnowledgeInput::QUEST,
            allowed: 0,
        };
        assert!(!decide(&mut call));
        call.allowed = 1;
        assert!(decide(&mut call));
    }
}
