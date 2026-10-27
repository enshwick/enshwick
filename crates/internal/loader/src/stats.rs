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
//! account in `mod-data/enshwick/worlds/<key>/stats.bin` and come back when
//! the player joins.

use combat::{combat, expire, follow};
use enshwick::Entity;

use crate::locks::Relock;
use crate::mod_index::ModIndex;
use crate::pinned;

mod equipment;
mod vitals;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use enshwick::abi::{
    AffixDefData, AffixRoll, Event, Header, ItemLine, PlayerData, StatChange, StatChangeFn,
    StatDefData, StatModifierData, StatModify, StatRef, StatSource, Status, Str,
};
use enshwick::affixes::{AffixDef, AffixKey, Candidate, ItemRarity, RollRules, lines_of, reroll};
use enshwick::stats::{ItemStats, Op, Stat, StatDef, aggregate, builtin, key_of};
use rustc_hash::FxHashMap;

use crate::host::{Loader, Registered};
use crate::ids::Space;
use crate::settings::LOADER_ID;

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
/// The file of a world's saved modifiers.
const FILE: &str = "stats.bin";
/// The damage types of a hit and the healing entry among them.
const TYPES: usize = 13;
const HEALING: usize = 7;

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
    buffs: FxHashMap<u32, Box<[StatModifierData]>>,
    handlers: Box<[Registered<StatChangeFn>]>,
    /// The built-in damage and resistance kinds: every type, then by type.
    damage: u32,
    resistance: u32,
    by_type: [Option<(u32, u32)>; TYPES],
    /// The built-in maximum kinds: health, stamina, mana.
    maxima: [u32; 3],
    /// Every mod's affixes, by full id, in the order they were defined.
    affixes: Box<[(Box<str>, AffixDef)]>,
}

/// What mods register during init.
#[derive(Debug, Default)]
struct Pending {
    kinds: Vec<Kind>,
    buffs: Vec<(u32, StatModifierData)>,
    handlers: Vec<Registered<StatChangeFn>>,
    affixes: Vec<(Box<str>, AffixDef)>,
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
    /// A mod asked to watch it.
    asked: bool,
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

/// A saved modifier, kept by the player's account, the mod's id and the
/// stat's id, so it reads back whatever the load order.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
struct Saved {
    player: u64,
    owner: String,
    key: u32,
    stat: String,
    op: u8,
    value: f32,
}

/// The server's watched entities and saved modifiers.
#[derive(Debug, Default)]
struct State {
    watched: FxHashMap<u32, Watched>,
    saved: Vec<Saved>,
    saved_changed: bool,
    world: Option<String>,
    next_expiry: Option<Instant>,
}

/// The values every reader sees: by entity, each kind's value, and the
/// damage factors of the entities that change hits.
#[derive(Debug, Default)]
struct Snapshot {
    values: FxHashMap<u32, Box<[f32]>>,
    combat: FxHashMap<u32, Combat>,
}

/// What an entity does to hits: factors on what it deals and takes, by
/// damage type.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Combat {
    deals: [f32; TYPES],
    takes: [f32; TYPES],
}

static PENDING: Mutex<Option<Pending>> = Mutex::new(None);
static SEALED: OnceLock<Sealed> = OnceLock::new();
static STATE: Mutex<Option<State>> = Mutex::new(None);
static SNAPSHOT: OnceLock<ArcSwap<Snapshot>> = OnceLock::new();
/// Bumped whenever an item's stats change, so every watched entity looks
/// again.
static ITEMS: AtomicU64 = AtomicU64::new(0);
/// Set when something asks for a look before the next round.
static LOOK: AtomicBool = AtomicBool::new(false);

fn snapshot() -> &'static ArcSwap<Snapshot> {
    SNAPSHOT.get_or_init(|| ArcSwap::from_pointee(Snapshot::default()))
}

fn pending<R>(f: impl FnOnce(&mut Pending) -> R) -> Result<R, Status> {
    if SEALED.get().is_some() {
        return Err(Status::TOO_LATE);
    }
    let mut pending = PENDING.relock();
    let pending = pending.get_or_insert_with(|| {
        let mut fresh = Pending::default();
        for (id, def) in builtin::ALL {
            let def = def.to_def();
            let text = def.text.clone().into_boxed_str();
            fresh.kinds.push(Kind {
                id: id.into(),
                def: Some((def, text, (ModIndex::LOADER).get())),
            });
        }
        fresh
    });
    pending.used = true;
    Ok(f(pending))
}

/// Whether `part` may be the name side of an id: 1 to 64 of `[a-z0-9_-]`.
fn is_part(part: &str) -> bool {
    (1..=64).contains(&part.len())
        && part
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

/// The full id of `id` asked for by the mod named `names` (its id first,
/// then its former ids), and whether the namespace is the mod's own. A bare
/// name takes the namespace where the mod's data lives: its first former
/// id, or else its id.
fn full_id(names: &[&str], id: &str) -> Option<(String, bool)> {
    let home = names.get(1).or_else(|| names.first())?;
    let (namespace, name) = id.split_once(':').unwrap_or((home, id));
    (enshwick::abi::is_valid_former_id(namespace) && is_part(name))
        .then(|| (format!("{namespace}:{name}"), names.contains(&namespace)))
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

/// Records `full` in its space; a name whose key another has turns mod
/// `owner` off with both named.
fn declare(loader: &Loader, owner: ModIndex, space: Space, full: &str) -> Result<(), Status> {
    if SEALED.get().is_some() {
        return Err(Status::TOO_LATE);
    }
    let key = key_of(full).0;
    let by = loader.mods.get(owner.get()).map_or("?", |m| m.id.as_str());
    let declared = crate::ids::with(|ids| {
        if space == Space::Stat {
            for (id, _) in builtin::ALL {
                let _builtin = ids.declare(LOADER_ID, Space::Stat, id, key_of(id).0);
            }
        }
        ids.declare(by, space, full, key)
    });
    declared.map_err(|why| {
        loader.turn_off(owner, &why);
        Status::REFUSED
    })
}

/// Defines or opens kind `id` for mod `owner`. During init only.
pub(crate) fn open(
    loader: &Loader,
    owner: ModIndex,
    id: &str,
    def: Option<StatDef>,
) -> Result<StatRef, Status> {
    let record = loader.mods.get(owner.get()).ok_or(Status::BAD_ARGS)?;
    let (full, own) = full_id(&record.names(), id).ok_or(Status::BAD_ARGS)?;
    declare(loader, owner, Space::Stat, &full)?;
    pending(|p| {
        let index = index_of(&mut p.kinds, &full);
        if let Some(def) = def {
            let kind = &mut p.kinds[index];
            if !own || kind.def.is_some() {
                return Err(Status::REFUSED);
            }
            let text = def.text.clone().into_boxed_str();
            kind.def = Some((def, text, owner.get()));
        }
        Ok(StatRef {
            index: u32::try_from(index).map_err(|_| Status::REFUSED)?,
            key: key_of(&full).0,
        })
    })?
}

/// Gives buff `buff` modifiers. During init only.
pub(crate) fn buff(buff: u32, modifiers: &[StatModifierData]) -> Result<(), Status> {
    if modifiers.iter().any(|m| Op::from_raw(m.op).is_none()) {
        return Err(Status::BAD_ARGS);
    }
    pending(|p| p.buffs.extend(modifiers.iter().map(|&m| (buff, m))))
}

/// Defines mod `owner`'s affix `id`. During init only.
pub(crate) fn open_affix(
    loader: &Loader,
    owner: ModIndex,
    id: &str,
    data: &AffixDefData,
) -> Result<StatRef, Status> {
    let record = loader.mods.get(owner.get()).ok_or(Status::BAD_ARGS)?;
    let (full, own) = full_id(&record.names(), id).ok_or(Status::BAD_ARGS)?;
    if !own {
        return Err(Status::REFUSED);
    }
    declare(loader, owner, Space::Affix, &full)?;
    pending(|p| {
        let kinds = &p.kinds;
        let stat = |index: u32| {
            let kind = kinds.get(index as usize)?;
            Some(Stat::from_ref(StatRef {
                index,
                key: key_of(&kind.id).0,
            }))
        };
        // SAFETY: the caller passes the definition's text and ranges for
        // the call.
        let def = unsafe { AffixDef::from_data(data, stat) }.ok_or(Status::BAD_ARGS)?;
        if p.affixes.iter().any(|(known, _)| **known == *full) {
            return Err(Status::REFUSED);
        }
        p.affixes.push((full.clone().into_boxed_str(), def));
        Ok(StatRef {
            index: u32::try_from(p.affixes.len() - 1).map_err(|_| Status::REFUSED)?,
            key: AffixKey::of(&full).0,
        })
    })?
}

/// The affix pool, in the order the affixes were defined.
fn pool(sealed: &Sealed) -> Vec<Candidate<'_>> {
    sealed
        .affixes
        .iter()
        .map(|(id, def)| Candidate {
            key: AffixKey::of(id),
            def,
        })
        .collect()
}

/// Rolls an item anew and keeps the result as its item stats; how many
/// affixes it rolled. On the server's game thread.
pub(crate) fn roll(loader: &Loader, roll: &AffixRoll) -> Result<u32, Status> {
    let sealed = SEALED.get().filter(|_| wanted()).ok_or(Status::MISSING)?;
    crate::world::server()?;
    if roll.pide == Entity(0) || roll.item == enshwick::ItemId::new(0) {
        return Err(Status::BAD_ARGS);
    }
    let category = crate::inventory::item_info(loader, roll.item.get())?.category;
    let current = crate::item_data::get_platform(roll.pide.0, roll.item.get())
        .and_then(|bytes| postcard::from_bytes::<ItemStats>(&bytes).ok())
        .unwrap_or_default();
    let rules = RollRules::new(roll.min, roll.max)
        .max_rarity(ItemRarity::from_raw(roll.max_rarity).unwrap_or(ItemRarity::Legendary));
    let once = roll.flags & AffixRoll::ONCE != 0;
    let next =
        reroll(&current, &pool(sealed), category, rules, roll.seed, once).ok_or(Status::REFUSED)?;
    if next == current {
        return Ok(0);
    }
    let bytes = postcard::to_allocvec(&next).map_err(|_| Status::REFUSED)?;
    crate::item_data::set_platform(roll.pide.0, roll.item.get(), &bytes)?;
    Ok(u32::try_from(next.affixes.len()).unwrap_or(0))
}

/// What the item `pide` holding `item` gives, a line per modifier.
pub(crate) fn item_lines(pide: u32, item: u32) -> Result<Vec<ItemLine>, Status> {
    let sealed = SEALED.get().filter(|_| wanted()).ok_or(Status::MISSING)?;
    let record = crate::item_data::get_platform(pide, item)
        .and_then(|bytes| postcard::from_bytes::<ItemStats>(&bytes).ok())
        .unwrap_or_default();
    Ok(lines_of(&record, &pool(sealed), |key| {
        sealed.by_key.get(&key.0).copied()
    }))
}

/// The text key and rarity of affix `affix`.
pub(crate) fn affix_info(affix: u32) -> Result<(Str, u8), Status> {
    let sealed = SEALED.get().ok_or(Status::MISSING)?;
    let (_, def) = sealed
        .affixes
        .get(usize::try_from(affix).map_err(|_| Status::BAD_ARGS)?)
        .ok_or(Status::MISSING)?;
    // The affixes live as long as the process, so their text does too.
    Ok((Str::new(&def.text), def.rarity.to_raw()))
}

/// Registers mod `owner`'s handler of changes. During init only.
pub(crate) fn on_changed(
    owner: ModIndex,
    callback: StatChangeFn,
    user: *mut core::ffi::c_void,
) -> Result<(), Status> {
    pending(|p| p.handlers.push(Registered::new(owner, callback, user)))
}

/// Whether any mod uses stats.
pub(crate) fn wanted() -> bool {
    SEALED.get().is_some_and(|s| !s.kinds.is_empty())
}

/// Seals the kinds; stats run only when a mod used them, and then they
/// keep item stats as item data of their own.
pub(crate) fn seal() {
    let pending = PENDING
        .relock()
        .take()
        .filter(|p| p.used)
        .unwrap_or_default();
    for kind in pending.kinds.iter().filter(|k| k.def.is_none()) {
        crate::log!(WARN, "stats: {} is opened but no mod defines it", kind.id);
    }
    let index = |id: &str| {
        pending
            .kinds
            .iter()
            .position(|k| &*k.id == id)
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
                || StatDef::additive("").range(0.0, 0.0),
                |(d, _, _)| d.clone(),
            )
        })
        .collect();
    let mut buffs: FxHashMap<u32, Vec<StatModifierData>> = FxHashMap::default();
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
        crate::item_data::open_platform(enshwick::stats::ITEM_KIND);
    }
    let _first = SEALED.set(sealed);
}

/// The definition and id of kind `stat`.
pub(crate) fn info(stat: u32) -> Result<(StatDefData, Str), Status> {
    let sealed = SEALED.get().ok_or(Status::MISSING)?;
    let kind = sealed
        .kinds
        .get(usize::try_from(stat).map_err(|_| Status::BAD_ARGS)?)
        .ok_or(Status::BAD_ARGS)?;
    let (def, text, _) = kind.def.as_ref().ok_or(Status::MISSING)?;
    let mut data = def.to_data();
    // The kinds live as long as the process, so their text does too.
    data.text = Str::new(text);
    Ok((data, Str::new(&kind.id)))
}

/// The value of kind `stat` on `entity`, from the snapshot; any thread.
pub(crate) fn value(entity: Entity, stat: u32) -> Result<f32, Status> {
    let sealed = SEALED.get().ok_or(Status::MISSING)?;
    let index = usize::try_from(stat).map_err(|_| Status::BAD_ARGS)?;
    let default = *sealed.defaults.get(index).ok_or(Status::BAD_ARGS)?;
    let values = snapshot().load();
    Ok(values
        .values
        .get(&entity.0)
        .and_then(|v| v.get(index))
        .copied()
        .unwrap_or(default))
}

/// Whether this process runs the server's world.
fn serves() -> bool {
    crate::world::server_world() != 0
}

fn with_state<R>(f: impl FnOnce(&mut State) -> R) -> R {
    let mut state = STATE.relock();
    f(state.get_or_insert_with(State::default))
}

/// Sets or removes mod `owner`'s modifier.
pub(crate) fn modify(loader: &Loader, owner: ModIndex, request: &StatModify) -> Result<(), Status> {
    let sealed = SEALED.get().ok_or(Status::MISSING)?;
    let stat = request.modifier.stat;
    if request.entity == Entity(0)
        || usize::try_from(stat).map_or(true, |s| s >= sealed.kinds.len())
    {
        return Err(Status::BAD_ARGS);
    }
    if !serves() {
        return Err(Status::REFUSED);
    }
    let remove = request.flags & StatModify::REMOVE != 0;
    let saved = request.flags & StatModify::SAVED != 0;
    if !remove && Op::from_raw(request.modifier.op).is_none() {
        return Err(Status::BAD_ARGS);
    }
    let until = match request.duration_ms {
        0 => None,
        ms => Some(
            Instant::now()
                .checked_add(Duration::from_millis(ms))
                .ok_or(Status::BAD_ARGS)?,
        ),
    };
    with_state(|state| {
        let watched = state.watched.entry(request.entity.0).or_default();
        if saved && !remove && watched.player.is_none() {
            return Err(Status::REFUSED);
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
            let mod_id = record.map(|m| m.id.clone()).unwrap_or_default();
            let stat_id = sealed.kinds[stat as usize].id.to_string();
            state.saved.retain(|s| {
                !(s.player == player
                    && record.is_some_and(|m| m.owns(&s.owner))
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

/// Watches or stops watching `entity` for a mod.
pub(crate) fn watch(entity: Entity, on: bool) -> Result<(), Status> {
    if SEALED.get().is_none() || entity == Entity(0) {
        return Err(Status::BAD_ARGS);
    }
    if !serves() {
        return Err(Status::REFUSED);
    }
    with_state(|state| {
        if on {
            let watched = state.watched.entry(entity.0).or_default();
            watched.asked = true;
            watched.dirty = true;
        } else if let Some(watched) = state.watched.get_mut(&entity.0) {
            watched.asked = false;
        }
    });
    Ok(())
}

/// An item's stats changed: every watched entity looks again.
pub(crate) fn items_changed() {
    ITEMS.fetch_add(1, Ordering::Relaxed);
    LOOK.store(true, Ordering::Relaxed);
}

/// A player's equipment changed: look before the next round.
pub(crate) fn equipment_changed() {
    LOOK.store(true, Ordering::Relaxed);
}

/// A player joined: watch them and give back their saved modifiers.
pub(crate) fn player_joined(loader: &Loader, player: &PlayerData) {
    let Some(sealed) = SEALED.get().filter(|_| wanted()) else {
        return;
    };
    with_state(|state| {
        let mut restored = Vec::new();
        for saved in state.saved.iter().filter(|s| s.player == player.key.get()) {
            let owner = loader.mods.iter().position(|m| m.owns(&saved.owner));
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
        let watched = state.watched.entry(player.entity.0).or_default();
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
        let Some(record) = crate::item_data::get_platform(pide, item)
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
    let mut buffs = crate::buffs::type_ids(loader, entity);
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
) -> Result<Vec<StatSource>, Status> {
    let sealed = SEALED.get().ok_or(Status::MISSING)?;
    crate::world::server()?;
    let held = with_state(|state| {
        state
            .watched
            .get(&entity.0)
            .map(|w| w.modifiers.clone())
            .unwrap_or_default()
    });
    let (mut list, _) = gather(loader, sealed, entity, &held);
    list.retain(|s| stat == u32::MAX || s.stat == stat);
    Ok(list)
}

/// Takes the values worked out, as `(entity, what it wore, values)`, into
/// `state`, and the changes into `changes`; whether any value moved.
fn take_values(
    sealed: &Sealed,
    state: &mut State,
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
                changes.push(StatChange {
                    header: Header::of::<StatChange>(),
                    entity: Entity(entity),
                    stat: StatRef {
                        index: u32::try_from(index).unwrap_or(u32::MAX),
                        key: key_of(&sealed.kinds[index].id).0,
                    },
                    old,
                    new,
                    reserved: 0,
                });
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
pub(crate) fn after_step(loader: &Loader, step: u64) {
    let Some(sealed) = SEALED.get().filter(|_| wanted()) else {
        return;
    };
    if crate::world::server().is_err() {
        return;
    }
    let round = step.is_multiple_of(LOOK_EVERY);
    let look = LOOK.swap(false, Ordering::Relaxed) || round;
    let players = if round {
        crate::world::players(loader).ok()
    } else {
        None
    };
    let mut forgot = false;
    let mut due = Vec::new();
    with_state(|state| {
        expire(state, Instant::now());
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
    let generation = ITEMS.load(Ordering::Relaxed);
    let mut results = Vec::new();
    for (entity, held, signature, dirty) in due {
        let (list, seen) = gather(loader, sealed, Entity(entity), &held);
        let seen = seen ^ generation.rotate_left(17);
        if dirty || seen != signature {
            results.push((entity, seen, work_out(sealed, &list)));
        }
    }
    if results.is_empty() && !forgot {
        return;
    }
    let mut changes = Vec::new();
    let published = with_state(|state| {
        let entities: Vec<u32> = results.iter().map(|r| r.0).collect();
        let moved = take_values(sealed, state, results, &mut changes);
        for entity in entities {
            if let Some(watched) = state.watched.get_mut(&entity) {
                raise_maxima(loader, sealed, Entity(entity), watched);
            }
        }
        (moved || forgot).then(|| publish(sealed, state))
    });
    if let Some(next) = published {
        snapshot().store(Arc::new(next));
    }
    for change in &changes {
        for handler in sealed.handlers.iter().filter(|h| h.live()) {
            // SAFETY: the handler lives in the mod's DLL, which stays loaded,
            // and takes the pointer it registered; the change is valid for
            // the call; the safe layer catches its panics.
            crate::profile::call(handler.owner, crate::profile::Feature::Stats, || unsafe {
                (handler.callback)(change, handler.user());
            });
        }
    }
}

/// The snapshot of `state`'s values: the entities whose values differ from
/// the defaults.
fn publish(sealed: &Sealed, state: &State) -> Snapshot {
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

/// The file of world `key`'s saved modifiers.
fn file(loader: &Loader, key: &str) -> PathBuf {
    loader
        .data_dir
        .join("enshwick")
        .join("worlds")
        .join(key)
        .join(FILE)
}

/// Follows the server's world: reads the saved modifiers once it has
/// loaded, writes them when it saves and when it shuts down.
pub(crate) fn world_event(loader: &Loader, event: Event) {
    if !wanted() || !serves() {
        return;
    }
    match event {
        Event::WORLD_LOADED => {
            let key = crate::saves::world_key(loader).map(str::to_owned);
            let saved: Vec<Saved> = key
                .as_deref()
                .and_then(|key| std::fs::read(file(loader, key)).ok())
                .and_then(|bytes| postcard::from_bytes(&bytes).ok())
                .unwrap_or_default();
            crate::log!(INFO, "stats: {} saved modifiers", saved.len());
            *STATE.relock() = Some(State {
                saved,
                world: key,
                ..State::default()
            });
            snapshot().store(Arc::new(Snapshot::default()));
        }
        Event::WORLD_SAVED | Event::SHUTDOWN_STARTED => save(loader),
        _ => {}
    }
}

/// The hosted game stopped: saves what changed and drops its world's
/// entities and values, so the next world starts clean.
pub(crate) fn forget() {
    if let Some(loader) = crate::host::loader() {
        save(loader);
    }
    *STATE.relock() = None;
    snapshot().store(Arc::new(Snapshot::default()));
}

fn save(loader: &Loader) {
    let Some((key, bytes)) = with_state(|state| {
        if !state.saved_changed {
            return None;
        }
        state.saved_changed = false;
        Some((
            state.world.clone()?,
            postcard::to_allocvec(&state.saved).ok()?,
        ))
    }) else {
        return;
    };
    let path = file(loader, &key);
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| {
            let fresh = path.with_extension("tmp");
            std::fs::write(&fresh, &bytes)?;
            std::fs::rename(&fresh, &path)
        });
    if let Err(error) = written {
        crate::log!(WARN, "stats: {} not written: {error}", path.display());
    }
}

mod combat;
pub(crate) use combat::on_hit;
#[cfg(test)]
mod tests;
