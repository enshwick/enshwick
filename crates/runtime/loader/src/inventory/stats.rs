//! Stats: kinds named `<mod>:<name>`, modifiers from worn items, buffs and
//! mods, and the values the server works out from them.
//!
//! Values are worked out on the game thread after a step in which a source
//! of a watched entity changed: a mod set or removed a modifier, a timed one
//! ran out, an item's stats changed, or the entity's worn items or buffs
//! differ from the last look (once a second, and at once on an equipment
//! event). The values go out as one snapshot swapped whole, so reads from
//! any thread, the damage sites' worker threads among them, take no lock.
//!
//! Players are always watched, and so is every entity a mod keeps a
//! modifier on or asks to watch. Saved modifiers stay with the player's
//! account in `<stem>-mod-data/enshwick/stats.bin` and come back when
//! the player joins.

use combat::{combat, expire, follow};
use enshwick_core::Entity;

use crate::engine::mod_index::ModIndex;
use crate::engine::pinned;
use enshwick_base::Relock;

mod affixes;
mod equipment;
mod error;
mod own;
mod saved;
mod shown;
mod vitals;

use affixes::{Affix, pool};
pub(crate) use affixes::{RollAffixesError, affix_info, item_lines, open_affix, roll};
pub(crate) use error::StatsError;
pub(crate) use own::{hello, own_value};
use saved::Saved;
pub(crate) use saved::world_event;
pub(crate) use shown::{group_kinds, item_line_texts, shown_kind};

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use enshwick_core::abi::{
    PlayerData, StatChange, StatChangeFn, StatModifier, StatModify, StatRef, StatSource, StatSpec,
    Str,
};
use enshwick_core::affixes::lines_of;
use enshwick_core::content::Id;
use enshwick_core::stats::{ItemStats, Op, StatDef, aggregate, builtin, key_of, name_key};
use rustc_hash::FxHashMap;

use crate::base::ids::Space;
use crate::engine::loader::{Loader, Registered};
use crate::engine::settings::LOADER_ID;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::attribute::HEALTH.id(),
    pinned::attribute::HEALTH_ADDER_SLOT.id(),
    pinned::attribute::HEALTH_MAX_SLOT.id(),
    pinned::attribute::MANA.id(),
    pinned::attribute::MANA_ADDER_SLOT.id(),
    pinned::attribute::MANA_MAX_SLOT.id(),
    pinned::attribute::STAMINA.id(),
    pinned::attribute::STAMINA_ADDER_SLOT.id(),
    pinned::attribute::STAMINA_MAX_SLOT.id(),
    pinned::ecs::ATTRIBUTE_COUNT.id(),
    pinned::ecs::LEVEL_DATA_OFFSET.id(),
    pinned::ecs::LEVEL_FLAGS.id(),
];

/// Steps between two looks at what watched entities wear and have.
const LOOK_EVERY: u64 = 30;
/// The damage types of a hit and the healing entry among them.
const TYPES: usize = enshwick_core::abi::DAMAGE_TYPES;
const HEALING: usize = enshwick_core::damage::DamageTypeNew::Healing.index();

/// One stat kind.
#[derive(Debug)]
struct Kind {
    id: Box<str>,
    /// The definition, and the mod that gave it; `None` while only opened.
    def: Option<(StatDef, Box<str>, usize)>,
}

/// The kinds once every mod is initialised.
#[derive(Debug)]
struct Sealed {
    kinds: Box<[Kind]>,
    by_key: FxHashMap<u32, u32>,
    defaults: Box<[f32]>,
    defs: Box<[StatDef]>,
    buffs: FxHashMap<u32, Box<[StatModifier]>>,
    handlers: Box<[Registered<StatChangeFn>]>,
    /// The built-in damage and resistance kinds: every type, then by type.
    damage: u32,
    resistance: u32,
    by_type: [Option<(u32, u32)>; TYPES],
    /// The built-in maximum kinds: health, stamina, mana.
    maxima: [u32; 3],
    /// Every mod's affixes, in the order they were defined.
    affixes: Box<[Affix]>,
}

/// What mods register during init.
#[derive(Debug, Default)]
struct Pending {
    kinds: Vec<Kind>,
    buffs: Vec<(u32, StatModifier)>,
    handlers: Vec<Registered<StatChangeFn>>,
    affixes: Vec<Affix>,
    used: bool,
}

/// A modifier a mod keeps on an entity.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Held {
    owner: ModIndex,
    key: u32,
    stat: u32,
    op: u8,
    value: f32,
    until: Option<Instant>,
    saved: bool,
}

/// A watched entity.
#[derive(Debug, Default)]
struct Watched {
    modifiers: Vec<Held>,
    /// The mods that ask to watch it, in index order.
    asked: Vec<ModIndex>,
    /// The account of the player it is.
    player: Option<u64>,
    /// What it wore and had at the last look, hashed.
    signature: u64,
    dirty: bool,
    /// The values worked out last; empty before the first time.
    values: Box<[f32]>,
    /// The points the loader added to the entity's maximum health, stamina
    /// and mana.
    applied: [i32; 3],
}

impl Watched {
    /// Adds mod `owner`'s ask to watch it, or takes that ask back; another
    /// mod's ask stays.
    fn ask(&mut self, owner: ModIndex, on: bool) {
        match (self.asked.binary_search(&owner), on) {
            (Err(at), true) => self.asked.insert(at, owner),
            (Ok(at), false) => {
                self.asked.remove(at);
            }
            _ => {}
        }
    }
}

/// The server's watched entities and saved modifiers.
#[derive(Debug, Default)]
struct World {
    watched: FxHashMap<u32, Watched>,
    saved: Vec<Saved>,
    saved_changed: bool,
    /// The world's key, where its saved modifiers go.
    key: Option<String>,
    next_expiry: Option<Instant>,
    /// The watched players whose shown values found no room on their link,
    /// sent again at the next step.
    values_owed: Vec<u32>,
}

/// The values every reader sees: by entity, each kind's value, and the
/// damage factors of the entities that change hits.
#[derive(Debug, Default)]
struct Snapshot {
    values: FxHashMap<u32, Box<[f32]>>,
    combat: FxHashMap<u32, Combat>,
}

impl Snapshot {
    /// Whether an entity of this snapshot changes hits.
    fn changes_hits(&self) -> bool {
        !self.combat.is_empty()
    }
}

/// What an entity does to hits: factors on what it deals and takes, by
/// damage type.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Combat {
    deals: [f32; TYPES],
    takes: [f32; TYPES],
}

static SEALED: OnceLock<Sealed> = OnceLock::new();

/// What the stats follow while the server runs a world.
#[derive(Debug)]
struct State {
    /// The kinds mods open and define in init, until the seal.
    pending: Mutex<Option<Pending>>,
    /// The served world's watched entities and saved modifiers.
    world: Mutex<Option<World>>,
    reads: HitReads,
    /// Bumped whenever an item's stats change, so every watched entity
    /// looks again.
    items: AtomicU64,
    /// Set when something asks for a look before the next round.
    look: AtomicBool,
}

/// What every hit reads on the damage workers, alone on its cache lines:
/// the game thread locks `world` every step, and mods bump `items` and
/// `look` from any thread.
#[derive(Debug)]
#[repr(align(128))]
struct HitReads {
    /// The values every reader sees, made at first use.
    snapshot: OnceLock<ArcSwap<Snapshot>>,
    /// Set while the snapshot holds hit factors, so a hit without any skips
    /// the snapshot.
    hits: AtomicBool,
}

impl State {
    const fn new() -> Self {
        Self {
            pending: Mutex::new(None),
            world: Mutex::new(None),
            reads: HitReads {
                snapshot: OnceLock::new(),
                hits: AtomicBool::new(false),
            },
            items: AtomicU64::new(0),
            look: AtomicBool::new(false),
        }
    }

    fn snapshot(&self) -> &ArcSwap<Snapshot> {
        self.reads
            .snapshot
            .get_or_init(|| ArcSwap::from_pointee(Snapshot::default()))
    }

    /// Makes `next` the snapshot every reader sees. Only the game thread
    /// swaps (the step, world events and forget phases), so `hits` follows
    /// the last snapshot.
    fn swap_in(&self, next: Snapshot) {
        self.reads
            .hits
            .store(next.changes_hits(), Ordering::Release);
        self.snapshot().store(Arc::new(next));
    }

    fn with_world<R>(&self, f: impl FnOnce(&mut World) -> R) -> R {
        let mut world = self.world.relock();
        f(world.get_or_insert_with(World::default))
    }

    /// Saves what changed in a hosted game that stopped and drops its
    /// world's entities, values and asked looks, so the next world starts
    /// clean; the item stats' generation goes on, as signatures fold it in.
    fn forget(&self) {
        self.save();
        *self.world.relock() = None;
        self.look.store(false, Ordering::Relaxed);
        self.swap_in(Snapshot::default());
    }
}

static STATE: State = State::new();

fn pending<R>(f: impl FnOnce(&mut Pending) -> R) -> Result<R, StatsError> {
    if SEALED.get().is_some() {
        return Err(StatsError::TooLate);
    }
    let mut pending = STATE.pending.relock();
    let pending = pending.get_or_insert_with(|| {
        let mut fresh = Pending::default();
        for (id, def) in builtin::ALL {
            let id = id.to_string();
            let text = name_key(&id).unwrap_or_default().into_boxed_str();
            fresh.kinds.push(Kind {
                id: id.into_boxed_str(),
                def: Some((def.to_def(), text, (ModIndex::LOADER).get())),
            });
        }
        fresh
    });
    pending.used = true;
    Ok(f(pending))
}

/// A content id a mod asks for.
#[derive(Debug)]
struct Asked {
    full: String,
    /// The text key that names it, see [`name_key`].
    text: Box<str>,
    /// Its namespace is the asking mod's own.
    own: bool,
}

/// The content id `id` asked for by the mod `mod_id`; `None` for one that
/// is not `<mod>:<name>` or has no [`name_key`].
fn full_id(mod_id: &str, id: &str) -> Option<Asked> {
    let (namespace, _) = id.split_once(':')?;
    let text = name_key(id)?.into_boxed_str();
    Some(Asked {
        full: id.to_owned(),
        text,
        own: namespace == mod_id,
    })
}

fn index_of(kinds: &mut Vec<Kind>, id: &str) -> usize {
    kinds.iter().position(|k| &*k.id == id).unwrap_or_else(|| {
        kinds.push(Kind {
            id: id.into(),
            def: None,
        });
        kinds.len() - 1
    })
}

/// Records `full` in its space; a name whose key another has is
/// [`StatsError::Taken`], which turns mod `owner` off with both named.
fn declare(loader: &Loader, owner: ModIndex, space: Space, full: &str) -> Result<(), StatsError> {
    if SEALED.get().is_some() {
        return Err(StatsError::TooLate);
    }
    let key = key_of(full).0;
    let by = loader.mods.get(owner.get()).map_or("?", |m| m.id.as_str());
    let declared = crate::base::ids::with(|ids| {
        if space == Space::Stat {
            for (id, _) in builtin::ALL {
                let id = id.to_string();
                let _builtin = ids.declare(LOADER_ID, Space::Stat, &id, key_of(&id).0);
            }
        }
        ids.declare(by, space, full, key)
    });
    Ok(declared?)
}

/// Defines or opens kind `id` for mod `owner`. During init only.
pub(crate) fn open(
    loader: &Loader,
    owner: ModIndex,
    id: &str,
    def: Option<StatDef>,
) -> Result<StatRef, StatsError> {
    let record = loader.mods.get(owner.get()).ok_or(StatsError::NoMod)?;
    let Asked { full, text, own } = full_id(&record.id, id).ok_or(StatsError::BadId)?;
    declare(loader, owner, Space::Stat, &full)?;
    pending(|p| {
        let index = index_of(&mut p.kinds, &full);
        if let Some(def) = def {
            let kind = &mut p.kinds[index];
            if !own {
                return Err(StatsError::NotOwn);
            }
            if kind.def.is_some() {
                return Err(StatsError::Defined);
            }
            kind.def = Some((def, text, owner.get()));
        }
        Ok(StatRef {
            index: u32::try_from(index).map_err(|_| StatsError::Full)?,
            key: key_of(&full).0,
        })
    })?
}

/// Gives buff `buff` modifiers. During init only.
pub(crate) fn buff(buff: u32, modifiers: &[StatModifier]) -> Result<(), StatsError> {
    if modifiers.iter().any(|m| Op::from_raw(m.op).is_none()) {
        return Err(StatsError::BadDef);
    }
    pending(|p| p.buffs.extend(modifiers.iter().map(|&m| (buff, m))))
}

/// Registers mod `owner`'s handler of changes. During init only.
pub(crate) fn on_changed(
    owner: ModIndex,
    callback: StatChangeFn,
    user: *mut core::ffi::c_void,
) -> Result<(), StatsError> {
    pending(|p| p.handlers.push(Registered::new(owner, callback, user)))
}

/// Whether any mod uses stats.
pub(crate) fn wanted() -> bool {
    SEALED.get().is_some_and(|s| !s.kinds.is_empty())
}

/// Seals the kinds; stats run only when a mod used them, and then they
/// keep item stats as item data of their own.
pub(crate) fn seal() {
    let pending = STATE
        .pending
        .relock()
        .take()
        .filter(|p| p.used)
        .unwrap_or_default();
    for kind in pending.kinds.iter().filter(|k| k.def.is_none()) {
        crate::log!(WARN, "stats: {} is opened but no mod defines it", kind.id);
    }
    let index = |id: Id| {
        let id = id.to_string();
        pending
            .kinds
            .iter()
            .position(|k| *k.id == *id)
            .and_then(|i| u32::try_from(i).ok())
            .unwrap_or(u32::MAX)
    };
    let mut by_type = [None; TYPES];
    for (slot, ids) in by_type.iter_mut().zip(builtin::BY_DAMAGE_TYPE) {
        *slot = ids.map(|(deal, take)| (index(deal), index(take)));
    }
    let defs: Box<[StatDef]> = pending
        .kinds
        .iter()
        .map(|k| {
            k.def.as_ref().map_or_else(
                || StatDef::additive().range(0.0, 0.0),
                |(d, _, _)| d.clone(),
            )
        })
        .collect();
    let mut buffs: FxHashMap<u32, Vec<StatModifier>> = FxHashMap::default();
    for (id, m) in pending.buffs {
        buffs.entry(id).or_default().push(m);
    }
    let sealed = Sealed {
        by_key: pending
            .kinds
            .iter()
            .enumerate()
            .filter_map(|(i, k)| Some((key_of(&k.id).0, u32::try_from(i).ok()?)))
            .collect(),
        defaults: defs.iter().map(|d| d.default).collect(),
        defs,
        damage: index(builtin::DAMAGE),
        maxima: [
            index(builtin::HEALTH_MAX),
            index(builtin::STAMINA_MAX),
            index(builtin::MANA_MAX),
        ],
        resistance: index(builtin::RESISTANCE),
        by_type,
        buffs: buffs
            .into_iter()
            .map(|(id, m)| (id, m.into_boxed_slice()))
            .collect(),
        handlers: pending.handlers.into_boxed_slice(),
        affixes: pending.affixes.into_boxed_slice(),
        kinds: pending.kinds.into_boxed_slice(),
    };
    if !sealed.kinds.is_empty() {
        crate::log!(
            INFO,
            "stats: {} kinds, {} buffs with modifiers",
            sealed.kinds.len(),
            sealed.buffs.len()
        );
        crate::inventory::item_data::open_platform(enshwick_core::stats::ITEM_KIND);
    }
    let _first = SEALED.set(sealed);
}

/// The definition and id of kind `stat`.
pub(crate) fn info(stat: u32) -> Result<(StatSpec, Str), StatsError> {
    let sealed = SEALED.get().ok_or(StatsError::NotRunning)?;
    let kind = sealed
        .kinds
        .get(usize::try_from(stat).map_err(|_| StatsError::BadStat)?)
        .ok_or(StatsError::BadStat)?;
    let (def, _, _) = kind.def.as_ref().ok_or(StatsError::Undefined)?;
    Ok((def.to_data(), Str::new(&kind.id)))
}

/// The value of kind `stat` on `entity`, from the snapshot; any thread.
pub(crate) fn value(entity: Entity, stat: u32) -> Result<f32, StatsError> {
    let sealed = SEALED.get().ok_or(StatsError::NotRunning)?;
    let index = usize::try_from(stat).map_err(|_| StatsError::BadStat)?;
    let default = *sealed.defaults.get(index).ok_or(StatsError::BadStat)?;
    let values = STATE.snapshot().load();
    Ok(values
        .values
        .get(&entity.get())
        .and_then(|v| v.get(index))
        .copied()
        .unwrap_or(default))
}

/// Whether this process runs the server's world.
fn serves() -> bool {
    crate::engine::world::server_world() != 0
}

/// Sets or removes mod `owner`'s modifier.
pub(crate) fn modify(
    loader: &Loader,
    owner: ModIndex,
    request: &StatModify,
) -> Result<(), StatsError> {
    let sealed = SEALED.get().ok_or(StatsError::NotRunning)?;
    let stat = request.modifier.stat;
    if request.entity == Entity::from_engine(0) {
        return Err(StatsError::BadModifier);
    }
    if usize::try_from(stat).map_or(true, |s| s >= sealed.kinds.len()) {
        return Err(StatsError::BadStat);
    }
    if !serves() {
        return Err(StatsError::NotServer);
    }
    let remove = request.flags & StatModify::REMOVE != 0;
    let saved = request.flags & StatModify::SAVED != 0;
    if !remove && Op::from_raw(request.modifier.op).is_none() {
        return Err(StatsError::BadModifier);
    }
    let until = match request.duration_ms {
        0 => None,
        ms => Some(
            Instant::now()
                .checked_add(Duration::from_millis(ms))
                .ok_or(StatsError::BadModifier)?,
        ),
    };
    STATE.with_world(|state| {
        let watched = state.watched.entry(request.entity.get()).or_default();
        if saved && !remove && watched.player.is_none() {
            return Err(StatsError::NotPlayer);
        }
        let same = |h: &Held| h.owner == owner && h.key == request.key && h.stat == stat;
        let was_saved = watched.modifiers.iter().any(|h| same(h) && h.saved);
        watched.modifiers.retain(|h| !same(h));
        if !remove {
            watched.modifiers.push(Held {
                owner,
                key: request.key,
                stat,
                op: request.modifier.op,
                value: request.modifier.value,
                until,
                saved,
            });
            watched.modifiers.sort_by_key(|h| (h.owner, h.key, h.stat));
        }
        watched.dirty = true;
        let player = watched.player;
        if let Some(until) = until {
            state.next_expiry = Some(state.next_expiry.map_or(until, |n| n.min(until)));
        }
        if (saved || was_saved)
            && let Some(player) = player
        {
            let record = loader.mods.get(owner.get());
            let mod_id = record.map_or_default(|m| m.id.clone());
            let stat_id = sealed.kinds[stat as usize].id.to_string();
            state.saved.retain(|s| {
                !(s.player == player
                    && record.is_some_and(|m| m.id == s.owner)
                    && s.key == request.key
                    && s.stat == stat_id)
            });
            if saved && !remove {
                state.saved.push(Saved {
                    player,
                    owner: mod_id,
                    key: request.key,
                    stat: stat_id,
                    op: request.modifier.op,
                    value: request.modifier.value,
                });
            }
            state.saved_changed = true;
        }
        Ok(())
    })
}

/// Watches or stops watching `entity` for mod `owner`; it stays watched
/// while any mod asks.
pub(crate) fn watch(owner: ModIndex, entity: Entity, on: bool) -> Result<(), StatsError> {
    if SEALED.get().is_none() || entity == Entity::from_engine(0) {
        return Err(StatsError::NothingToWatch);
    }
    if !serves() {
        return Err(StatsError::NotServer);
    }
    STATE.with_world(|state| {
        if on {
            let watched = state.watched.entry(entity.get()).or_default();
            watched.ask(owner, true);
            watched.dirty = true;
        } else if let Some(watched) = state.watched.get_mut(&entity.get()) {
            watched.ask(owner, false);
        }
    });
    Ok(())
}

/// An item's stats changed: every watched entity looks again.
pub(crate) fn items_changed() {
    STATE.items.fetch_add(1, Ordering::Relaxed);
    STATE.look.store(true, Ordering::Relaxed);
}

/// A player's equipment changed: look before the next round.
pub(crate) fn equipment_changed() {
    STATE.look.store(true, Ordering::Relaxed);
}

/// A player joined: watch them and give back their saved modifiers.
pub(crate) fn player_joined(loader: &Loader, player: &PlayerData) {
    let Some(sealed) = SEALED.get().filter(|_| wanted()) else {
        return;
    };
    STATE.with_world(|state| {
        let mut restored = Vec::new();
        for saved in state.saved.iter().filter(|s| s.player == player.key.get()) {
            let owner = loader.mods.iter().position(|m| m.id == saved.owner);
            let stat = sealed.by_key.get(&key_of(&saved.stat).0).copied();
            if let (Some(owner), Some(stat)) = (owner, stat) {
                restored.push(Held {
                    owner: ModIndex::new(owner),
                    key: saved.key,
                    stat,
                    op: saved.op,
                    value: saved.value,
                    until: None,
                    saved: true,
                });
            }
        }
        let watched = state.watched.entry(player.entity.get()).or_default();
        watched.player = Some(player.key.get());
        watched.modifiers.retain(|h| !h.saved);
        watched.modifiers.extend(restored);
        watched.modifiers.sort_by_key(|h| (h.owner, h.key, h.stat));
        watched.dirty = true;
    });
}

/// One modifier counting toward an entity's stat, as the sources call
/// lists it.
fn source(kind: u8, op: u8, stat: u32, value: f32, id: u32, owner: u32) -> StatSource {
    StatSource {
        kind,
        op,
        reserved: 0,
        stat,
        value,
        id,
        owner,
    }
}

/// What `entity` wears and has, in source order, with a hash of it.
fn gather(
    loader: &Loader,
    sealed: &Sealed,
    entity: Entity,
    held: &[Held],
) -> (Vec<StatSource>, u64) {
    let mut out = Vec::new();
    let mut hash = rustc_hash::FxHasher::default();
    let mut items = equipment::worn(loader, entity);
    items.sort_unstable();
    for &(pide, item) in &items {
        core::hash::Hasher::write_u64(&mut hash, (u64::from(pide) << 32) | u64::from(item));
        let Some(record) = crate::inventory::item_data::get_platform(pide, item)
            .and_then(|bytes| postcard::from_bytes::<ItemStats>(&bytes).ok())
        else {
            continue;
        };
        for line in lines_of(&record, &pool(sealed), |key| {
            sealed.by_key.get(&key.0).copied()
        }) {
            let m = line.modifier;
            out.push(source(StatSource::ITEM, m.op, m.stat, m.value, pide, item));
        }
    }
    let mut buffs = crate::world::buffs::type_ids(loader, entity);
    buffs.sort_unstable();
    buffs.dedup();
    for &buff in &buffs {
        core::hash::Hasher::write_u32(&mut hash, buff);
        for m in sealed.buffs.get(&buff).into_iter().flatten() {
            out.push(source(StatSource::BUFF, m.op, m.stat, m.value, buff, 0));
        }
    }
    for h in held {
        out.push(source(
            StatSource::ENTITY,
            h.op,
            h.stat,
            h.value,
            h.key,
            u32::try_from(h.owner.get()).unwrap_or(u32::MAX),
        ));
    }
    (out, core::hash::Hasher::finish(&hash))
}

/// Every kind's value under `sources`.
fn work_out(sealed: &Sealed, sources: &[StatSource]) -> Box<[f32]> {
    sealed
        .defs
        .iter()
        .enumerate()
        .map(|(index, def)| {
            let stat = u32::try_from(index).unwrap_or(u32::MAX);
            aggregate(
                def,
                sources
                    .iter()
                    .filter(|s| s.stat == stat)
                    .filter_map(|s| Some((Op::from_raw(s.op)?, s.value))),
            )
        })
        .collect()
}

/// The modifiers toward `stat` (every stat for `u32::MAX`) on `entity`, in
/// the order they apply; on the game thread.
pub(crate) fn sources(
    loader: &Loader,
    entity: Entity,
    stat: u32,
) -> Result<Vec<StatSource>, StatsError> {
    let sealed = SEALED.get().ok_or(StatsError::NotRunning)?;
    crate::engine::world::server()?;
    let held = STATE.with_world(|state| {
        state
            .watched
            .get(&entity.get())
            .map_or_default(|w| w.modifiers.clone())
    });
    let (mut list, _) = gather(loader, sealed, entity, &held);
    list.retain(|s| stat == u32::MAX || s.stat == stat);
    Ok(list)
}

/// Takes the values worked out, as `(entity, what it wore, values)`, into
/// `state`, and the changes into `changes`; whether any value moved.
fn take_values(
    sealed: &Sealed,
    state: &mut World,
    results: Vec<(u32, u64, Box<[f32]>)>,
    changes: &mut Vec<StatChange>,
) -> bool {
    let mut moved = false;
    for (entity, seen, values) in results {
        let Some(watched) = state.watched.get_mut(&entity) else {
            continue;
        };
        watched.signature = seen;
        watched.dirty = false;
        for (index, (&new, def)) in values.iter().zip(&sealed.defaults).enumerate() {
            let old = watched.values.get(index).copied().unwrap_or(*def);
            if old.to_bits() != new.to_bits() {
                changes.push(enshwick_abi::headed!(StatChange {
                    entity: Entity::from_engine(entity),
                    stat: StatRef {
                        index: u32::try_from(index).unwrap_or(u32::MAX),
                        key: key_of(&sealed.kinds[index].id).0,
                    },
                    old,
                    new,
                    reserved: 0,
                }));
            }
        }
        moved |= *watched.values != *values;
        watched.values = values;
    }
    moved
}

/// Brings the points the loader added to `entity`'s maxima to its built-in
/// maximum kinds' values.
fn raise_maxima(loader: &Loader, sealed: &Sealed, entity: Entity, watched: &mut Watched) {
    let pools = [
        vitals::Pool::Health,
        vitals::Pool::Stamina,
        vitals::Pool::Mana,
    ];
    for ((pool, &stat), applied) in pools.iter().zip(&sealed.maxima).zip(&mut watched.applied) {
        let Some(&value) = watched.values.get(stat as usize) else {
            continue;
        };
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the kinds' range keeps the points far inside i32"
        )]
        let target = value.round() as i32;
        let delta = target.saturating_sub(*applied);
        if delta != 0 && vitals::add_max(loader, entity, *pool, delta) {
            *applied = target;
        }
    }
}

/// The step's work on the game thread: drops what ran out, looks at the
/// watched entities when due, works out the values that may have changed
/// and tells the mods.
pub(crate) fn after_step(loader: &Loader, step: u64, now: Instant) {
    let Some(sealed) = SEALED.get().filter(|_| wanted()) else {
        return;
    };
    if crate::engine::world::server().is_err() {
        return;
    }
    let round = step.is_multiple_of(LOOK_EVERY);
    let look = STATE.look.swap(false, Ordering::Relaxed) || round;
    let players = if round {
        crate::engine::world::players(loader).ok()
    } else {
        None
    };
    let mut forgot = false;
    let mut owing = false;
    let mut due = Vec::new();
    STATE.with_world(|state| {
        expire(state, now);
        owing = !state.values_owed.is_empty();
        if let Some(players) = &players {
            forgot = follow(loader, state, players);
        }
        for (&entity, watched) in &state.watched {
            if watched.dirty || look {
                due.push((
                    entity,
                    watched.modifiers.clone(),
                    watched.signature,
                    watched.dirty,
                ));
            }
        }
    });
    let generation = STATE.items.load(Ordering::Relaxed);
    let mut results = Vec::new();
    for (entity, held, signature, dirty) in due {
        let (list, seen) = gather(loader, sealed, Entity::from_engine(entity), &held);
        let seen = seen ^ generation.rotate_left(17);
        if dirty || seen != signature {
            results.push((entity, seen, work_out(sealed, &list)));
        }
    }
    if results.is_empty() && !forgot && !owing {
        return;
    }
    let mut changes = Vec::new();
    let published = STATE.with_world(|state| {
        let entities: Vec<u32> = results.iter().map(|r| r.0).collect();
        let moved = take_values(sealed, state, results, &mut changes);
        own::send_changed(sealed, state, &changes);
        for entity in entities {
            if let Some(watched) = state.watched.get_mut(&entity) {
                raise_maxima(loader, sealed, Entity::from_engine(entity), watched);
            }
        }
        (moved || forgot).then(|| publish(sealed, state))
    });
    if let Some(next) = published {
        STATE.swap_in(next);
    }
    for change in &changes {
        let _none: Option<(_, ())> = crate::engine::filters::ask(
            sealed.handlers.iter(),
            crate::engine::profile::Feature::Stats,
            |handler| {
                // SAFETY: the handler lives in the mod's DLL, which stays
                // loaded, and takes the pointer it registered; the change is
                // valid for the call; by the callback contract of `HostApi`
                // it does not unwind into the loader.
                handler.call(|handler, user| unsafe { handler(change, user) });
                core::ops::ControlFlow::Continue(())
            },
        );
    }
}

/// The snapshot of `state`'s values: the entities whose values differ from
/// the defaults.
fn publish(sealed: &Sealed, state: &World) -> Snapshot {
    let mut next = Snapshot::default();
    for (&entity, watched) in &state.watched {
        if watched.values.is_empty() || *watched.values == *sealed.defaults {
            continue;
        }
        if let Some(row) = combat(sealed, &watched.values) {
            next.combat.insert(entity, row);
        }
        next.values.insert(entity, watched.values.clone());
    }
    next
}

/// The hosted game stopped: saves what changed and drops its world's
/// entities and values, so the next world starts clean.
pub(crate) fn forget() {
    STATE.forget();
}

mod combat;
pub(crate) use combat::{changes_hits, on_hit};
#[cfg(test)]
mod tests;
