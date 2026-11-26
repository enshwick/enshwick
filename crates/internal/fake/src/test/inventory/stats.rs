//! The fake loader's stats: kinds, modifiers on entities and buffs, the
//! items each entity wears, and values worked out on every read.

use crate::test::table::base::args;
use crate::test::table::entries;
use core::ffi::c_void;
use std::collections::{BTreeMap, BTreeSet};

use enshwick_core::abi::{
    AffixRoll, AffixSpec, ItemLine, StatChange, StatChangeFn, StatModifier, StatModify, StatRef,
    StatSource, StatSpec, Status, Str,
};
use enshwick_core::affixes::{
    AffixDef, AffixKey, Candidate, ItemRarity, RollRules, lines_of, reroll,
};
use enshwick_core::inventory::ItemInstance;
use enshwick_core::stats::{
    ItemStats, Op, Stat, StatDef, StatKey, aggregate, builtin, key_of, name_key,
};

use crate::test::table::{Registered, with};

/// One kind: its full id, kept for the process, and its definition.
pub(crate) struct Kind {
    id: Box<str>,
    def: Option<(StatDef, Box<str>)>,
}

/// An entity modifier by entity, key and stat: operation, value, flags and
/// duration.
type Modifiers = BTreeMap<(u32, u32, u32), (u8, f32, u32, u64)>;

/// The fake's stats.
#[derive(Default)]
pub(crate) struct FakeStats {
    kinds: Vec<Kind>,
    modifiers: Modifiers,
    buffs: Vec<(u32, StatModifier)>,
    handlers: Vec<Registered<StatChangeFn>>,
    /// The entities a modifier keeps watched.
    watched: BTreeSet<u32>,
    /// Each mod's asks to watch an entity, by entity and mod handle.
    asked: BTreeSet<(u32, usize)>,
    /// What each entity wears.
    pub(crate) worn: BTreeMap<u32, Vec<ItemInstance>>,
    /// The values the last settle raised, by entity and stat.
    settled: BTreeMap<(u32, u32), f32>,
    /// Affixes by full id with their text keys, in the order they were
    /// defined.
    affixes: Vec<(Box<str>, AffixDef, Box<str>)>,
    /// Whether a mod used stats during init, which alone makes the loader
    /// want them and arm the hit site for them.
    used_in_init: bool,
}

impl FakeStats {
    /// Whether the loader wants stats: a mod used them during init.
    pub(crate) fn wanted(&self) -> bool {
        self.used_in_init
    }

    fn with_builtins() -> Self {
        let mut stats = Self::default();
        for (id, def) in builtin::ALL {
            let id = id.to_string();
            let text = text_of(&id);
            stats.kinds.push(Kind {
                id: id.into_boxed_str(),
                def: Some((def.to_def(), text)),
            });
        }
        stats
    }
}

/// The content id `id`, as the loader takes it: `<mod>:<name>` with a
/// [`name_key`].
fn full(id: &str) -> Option<String> {
    name_key(id).map(|_| id.to_owned())
}

/// The text key the loader derives for full id `id`.
fn text_of(id: &str) -> Box<str> {
    name_key(id).unwrap_or_default().into_boxed_str()
}

/// Whether the full id `id` is in the namespace of the mod the fake runs:
/// its manifest's id once its init ran, else [`MOD_ID`](crate::test::MOD_ID),
/// and `me` is that mod.
fn is_own(s: &crate::test::table::State, me: enshwick_core::abi::ModHandle, id: &str) -> bool {
    if crate::test::table::base::mods::in_another_s_namespace(s, me, id) {
        return false;
    }
    let own = s
        .base
        .manifest
        .as_ref()
        .map_or(crate::test::MOD_ID, |manifest| manifest.id.as_str());
    id.split_once(':')
        .is_some_and(|(namespace, _)| namespace == own)
}

fn index_of(stats: &mut FakeStats, id: &str) -> u32 {
    let at = stats
        .kinds
        .iter()
        .position(|k| &*k.id == id)
        .unwrap_or_else(|| {
            stats.kinds.push(Kind {
                id: id.into(),
                def: None,
            });
            stats.kinds.len() - 1
        });
    u32::try_from(at).unwrap_or(u32::MAX)
}

fn state_stats(s: &mut crate::test::table::State) -> &mut FakeStats {
    let in_init = !s.base.closed();
    if s.inventory.stats.kinds.is_empty() {
        s.inventory.stats = FakeStats::with_builtins();
    }
    s.inventory.stats.used_in_init |= in_init;
    &mut s.inventory.stats
}

entries! {
    pub(crate) fn define_stat(
        me as Mod,
        id: Str as args::Text,
        def: *const StatSpec as args::MaybeHeaded,
        out: *mut StatRef as args::Out,
    ) -> Status {
        // SAFETY: the ABI lends `id`'s bytes for the call, and the result does
        // not outlive it.
        let Some(id) = unsafe { id.as_str() }.and_then(full) else {
            return Status::BAD_ARGS;
        };
        let def = if def.is_null() {
            None
        } else {
            // A definition short of what every writer knows is malformed, as the
            // loader has it, not absent.
            let Some(def) = crate::test::table::get_headed(def) else {
                return Status::BAD_ARGS;
            };
            Some(def)
        };
        let parsed = match &def {
            Some(data) => match StatDef::from_data(data) {
                Some(def) => Some(def),
                None => return Status::BAD_ARGS,
            },
            None => None,
        };
        if crate::test::table::late() {
            return Status::TOO_LATE;
        }
        let opened = with(Err(Status::MISSING), |s| {
            let unexported = parsed.is_none()
                && s.base.manifest.as_ref().is_some_and(|manifest| {
                    manifest.unexported(
                        enshwick_core::schema_section::Kind::Stat,
                        &id,
                        &s.host.base.exports,
                    )
                });
            if unexported {
                crate::test::table::base::mods::warn_unexported(
                    s,
                    enshwick_core::schema_section::Kind::Stat,
                    &id,
                );
                return Err(Status::REFUSED);
            }
            let own = is_own(s, me, &id);
            let stats = state_stats(s);
            let index = index_of(stats, &id);
            if let Some(def) = parsed {
                let kind = &mut stats.kinds[index as usize];
                if kind.def.is_some() || !own {
                    return Err(Status::REFUSED);
                }
                kind.def = Some((def, text_of(&id)));
            }
            Ok(index)
        });
        match opened {
            Ok(index) if !out.is_null() => {
                // SAFETY: the match guard refused null; the safe layer passes an
                // aligned `StatRef` it owns.
                unsafe {
                    out.write(StatRef {
                        index,
                        key: key_of(&id).0,
                    });
                }
                Status::OK
            }
            Ok(_) => Status::BAD_ARGS,
            Err(status) => status,
        }
    }

    pub(crate) fn stat_info(
        me as Caller,
        stat: u32,
        def: *mut StatSpec as args::HeadedOut,
        id: *mut Str as args::Out,
    ) -> Status {
        let found = with(None, |s| {
            let kind = state_stats(s).kinds.get(stat as usize)?;
            let (def, _) = kind.def.as_ref()?;
            // The fake keeps the id while installed, as the loader keeps it for
            // the process.
            Some((def.to_data(), Str::new(&kind.id)))
        });
        let Some((data, name)) = found else {
            return Status::MISSING;
        };
        let written = crate::test::table::put_headed(def, data);
        if written != Status::OK {
            return written;
        }
        crate::test::table::put(id, name)
    }
}

/// Every modifier toward `stat` (all for `u32::MAX`) on `entity`, in
/// source order.
fn sources(s: &mut crate::test::table::State, entity: u32, stat: u32) -> Vec<StatSource> {
    let worn = state_stats(s)
        .worn
        .get(&entity)
        .cloned()
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut items = worn;
    items.sort_unstable();
    for item in items {
        let record = record(s, item);
        for line in item_lines_of(s, &record) {
            out.push(StatSource {
                kind: StatSource::ITEM,
                op: line.modifier.op,
                reserved: 0,
                stat: line.modifier.stat,
                value: line.modifier.value,
                id: item.pide.get(),
                owner: item.item.get(),
            });
        }
    }
    let mut buffs: Vec<u32> = s
        .world
        .buffs
        .get(&entity)
        .map_or_default(|on| on.iter().map(|b| b.id.get()).collect());
    buffs.sort_unstable();
    for buff in buffs {
        for (_, m) in s.inventory.stats.buffs.iter().filter(|(id, _)| *id == buff) {
            out.push(StatSource {
                kind: StatSource::BUFF,
                op: m.op,
                reserved: 0,
                stat: m.stat,
                value: m.value,
                id: buff,
                owner: 0,
            });
        }
    }
    for (&(_, key, at), &(op, value, _, _)) in s
        .inventory
        .stats
        .modifiers
        .range((entity, 0, 0)..=(entity, u32::MAX, u32::MAX))
    {
        out.push(StatSource {
            kind: StatSource::ENTITY,
            op,
            reserved: 0,
            stat: at,
            value,
            id: key,
            owner: 0,
        });
    }
    out.retain(|m| stat == u32::MAX || m.stat == stat);
    out
}

fn value(s: &mut crate::test::table::State, entity: u32, stat: u32) -> Option<f32> {
    let def = state_stats(s)
        .kinds
        .get(stat as usize)?
        .def
        .as_ref()
        .map(|(def, _)| def.clone());
    let Some(def) = def else {
        return Some(0.0);
    };
    let mods = sources(s, entity, stat);
    Some(aggregate(
        &def,
        mods.iter()
            .filter_map(|m| Some((Op::from_raw(m.op)?, m.value))),
    ))
}

entries! {
    pub(crate) fn stat_value(
        me as Caller,
        entity: enshwick_core::Entity,
        stat: u32,
        out: *mut f32 as args::Out,
    ) -> Status {
        match with(None, |s| value(s, entity.get(), stat)) {
            Some(v) if !out.is_null() => {
                // SAFETY: the match guard refused null; the safe layer passes an
                // aligned `f32` it owns.
                unsafe { out.write(v) };
                Status::OK
            }
            Some(_) => Status::BAD_ARGS,
            None => Status::MISSING,
        }
    }

    pub(crate) fn stat_modify(me as Mod, request: *const StatModify as args::Headed) -> Status {
        let Some(request) = crate::test::table::get_headed(request) else {
            return Status::BAD_ARGS;
        };
        with(Status::MISSING, |s| {
            let saved = request.flags & StatModify::SAVED != 0;
            if saved
                && !s
                    .host
                    .base
                    .players
                    .iter()
                    .any(|p| p.entity == request.entity)
            {
                return Status::REFUSED;
            }
            let stats = state_stats(s);
            let at = (request.entity.get(), request.key, request.modifier.stat);
            if request.flags & StatModify::REMOVE != 0 {
                stats.modifiers.remove(&at);
            } else {
                let m = request.modifier;
                stats
                    .modifiers
                    .insert(at, (m.op, m.value, request.flags, request.duration_ms));
            }
            stats.watched.insert(request.entity.get());
            Status::OK
        })
    }

    pub(crate) fn stat_buff(
        me as Caller,
        buff: u32,
        [modifiers: *const StatModifier, count: u32] as args::List,
    ) -> Status {
        if modifiers.is_null() && count != 0 {
            return Status::BAD_ARGS;
        }
        let list = if count == 0 {
            Vec::new()
        } else {
            // SAFETY: the check before refused null with a nonzero `count`; the
            // safe layer passes `count` aligned modifiers alive for the call.
            unsafe { core::slice::from_raw_parts(modifiers, count as usize) }.to_vec()
        };
        crate::test::table::with_open(|s| {
            state_stats(s)
                .buffs
                .extend(list.into_iter().map(|m| (buff, m)));
            Status::OK
        })
    }

    pub(crate) fn stats_changed(
        me as Mod,
        callback: Option<StatChangeFn> as args::Callback,
        user: *mut c_void,
    ) -> Status {
        let Some(callback) = callback else {
            return Status::BAD_ARGS;
        };
        crate::test::table::with_open(|s| {
            state_stats(s)
                .handlers
                .push(Registered::new(callback, user, me));
            Status::OK
        })
    }

    pub(crate) fn stat_sources(
        me as Caller,
        entity: enshwick_core::Entity,
        stat: u32,
        [out: *mut StatSource, capacity: u32, count: *mut u32] as args::ListOut,
    ) -> Status {
        let list = with(Vec::new(), |s| sources(s, entity.get(), stat));
        crate::test::table::put_list(&list, out, capacity, count)
    }

    pub(crate) fn stat_watch(
        me as Mod,
        entity: enshwick_core::Entity,
        watch: u8,
    ) -> Status {
        with(Status::MISSING, |s| {
            // As the loader: nothing to watch before the stats are sealed
            // or for no entity, and only a game's server watches.
            if !s.base.closed() || entity.get() == 0 {
                return Status::BAD_ARGS;
            }
            if !s.host.base.serving() {
                return Status::UNAVAILABLE;
            }
            let stats = state_stats(s);
            let ask = (entity.get(), me.0);
            if watch == 0 {
                stats.asked.remove(&ask);
            } else {
                stats.asked.insert(ask);
            }
            Status::OK
        })
    }
}

/// Works out every watched entity's values and raises the handlers for each
/// that changed since the last settle, as the loader does after a step;
/// how many changes it raised.
pub(crate) fn settle() -> usize {
    let (changes, handlers) = with((Vec::new(), Vec::new()), |s| {
        let stats = state_stats(s);
        let mut entities: BTreeSet<u32> = stats.watched.clone();
        entities.extend(stats.asked.iter().map(|&(entity, _)| entity));
        entities.extend(stats.worn.keys());
        entities.extend(s.host.base.players.iter().map(|p| p.entity.get()));
        let kinds = u32::try_from(s.inventory.stats.kinds.len()).unwrap_or(0);
        let mut changes = Vec::new();
        for entity in entities {
            for stat in 0..kinds {
                let Some(new) = value(s, entity, stat) else {
                    continue;
                };
                let default = s.inventory.stats.kinds[stat as usize]
                    .def
                    .as_ref()
                    .map_or(0.0, |(d, _)| d.default);
                let old = s
                    .inventory
                    .stats
                    .settled
                    .get(&(entity, stat))
                    .copied()
                    .unwrap_or(default);
                if old.to_bits() != new.to_bits() {
                    s.inventory.stats.settled.insert((entity, stat), new);
                    let key = key_of(&s.inventory.stats.kinds[stat as usize].id).0;
                    changes.push(enshwick_core::headed!(StatChange {
                        entity: enshwick_core::Entity::from_engine(entity),
                        stat: StatRef { index: stat, key },
                        old,
                        new,
                        reserved: 0,
                    }));
                }
            }
        }
        (changes, s.inventory.stats.handlers.clone())
    });
    for change in &changes {
        for handler in &handlers {
            // SAFETY: `change` is a live `StatChange` borrowed for the call, and
            // `user` is the pointer the mod registered with `callback`, as
            // `StatChangeFn` expects; the thread that drives the fake stands
            // for the game thread.
            handler.run(|callback, user| unsafe { callback(change, user) });
        }
    }
    changes.len()
}

/// The modifiers the mod keeps: `(entity, key hash, stat index, op, value,
/// flags, duration in ms)`.
pub(crate) fn modifiers() -> Vec<(u32, u32, u32, u8, f32, u32, u64)> {
    with(Vec::new(), |s| {
        s.inventory
            .stats
            .modifiers
            .iter()
            .map(|(&(e, k, st), &(op, v, f, d))| (e, k, st, op, v, f, d))
            .collect()
    })
}

/// The item stats `item` keeps.
fn record(s: &crate::test::table::State, item: ItemInstance) -> ItemStats {
    s.content
        .item_kinds
        .iter()
        .position(|k| k == enshwick_core::stats::ITEM_KIND)
        .and_then(|kind| {
            s.content
                .item_values
                .get(&(u32::try_from(kind).ok()?, item.pide.get()))
        })
        .filter(|(held, _)| *held == item.item.get())
        .and_then(|(_, bytes)| postcard::from_bytes::<ItemStats>(bytes).ok())
        .unwrap_or_default()
}

fn pool(stats: &FakeStats) -> Vec<Candidate<'_>> {
    stats
        .affixes
        .iter()
        .map(|(id, def, _)| Candidate::new(AffixKey::of(id), def))
        .collect()
}

fn item_lines_of(s: &crate::test::table::State, record: &ItemStats) -> Vec<ItemLine> {
    let kinds = &s.inventory.stats.kinds;
    let stat = |key: StatKey| {
        kinds
            .iter()
            .position(|k| key_of(&k.id) == key)
            .and_then(|i| u32::try_from(i).ok())
    };
    lines_of(record, &pool(&s.inventory.stats), stat)
}

entries! {
    pub(crate) fn define_affix(
        me as Mod,
        id: Str as args::Text,
        def: *const AffixSpec as args::Headed,
        out: *mut StatRef as args::Out,
    ) -> Status {
        // SAFETY: the ABI lends `id`'s bytes for the call, and the result does
        // not outlive it.
        let Some(id) = unsafe { id.as_str() }.and_then(full) else {
            return Status::BAD_ARGS;
        };
        let Some(data) = crate::test::table::get_headed(def) else {
            return Status::BAD_ARGS;
        };
        // As the loader: the id and its owner, then the phase, then the
        // definition.
        // Without a fake installed the open below answers MISSING.
        if !with(true, |s| is_own(s, me, &id)) {
            return Status::REFUSED;
        }
        if crate::test::table::late() {
            return Status::TOO_LATE;
        }
        let opened = with(Err(Status::MISSING), |s| {
            let stats = state_stats(s);
            let stat = |index: u32| {
                let kind = stats.kinds.get(index as usize)?;
                Some(Stat::from_ref(StatRef {
                    index,
                    key: key_of(&kind.id).0,
                }))
            };
            // SAFETY: the ABI lends the definition's group and ranges for the
            // call.
            let Some(parsed) = (unsafe { AffixDef::from_data(&data, stat) }) else {
                return Err(Status::BAD_ARGS);
            };
            if stats.affixes.iter().any(|(known, _, _)| **known == *id) {
                return Err(Status::REFUSED);
            }
            stats
                .affixes
                .push((id.clone().into_boxed_str(), parsed, text_of(&id)));
            u32::try_from(stats.affixes.len() - 1).map_err(|_| Status::REFUSED)
        });
        match opened {
            Ok(index) => crate::test::table::put(
                out,
                StatRef {
                    index,
                    key: AffixKey::of(&id).0,
                },
            ),
            Err(status) => status,
        }
    }

    pub(crate) fn affix_roll(
        me as Caller,
        roll: *const AffixRoll as args::Headed,
        count: *mut u32 as args::Out,
    ) -> Status {
        let Some(roll) = crate::test::table::get_headed(roll) else {
            return Status::BAD_ARGS;
        };
        let item = ItemInstance {
            pide: roll.pide,
            item: roll.item,
        };
        let next = with(Err(Status::MISSING), |s| {
            let category = s
                .host
                .content
                .items
                .get(&roll.item.get())
                .map_or(0, |i| i.category);
            let rules = RollRules::new(roll.min, roll.max)
                .max_rarity(ItemRarity::from_raw(roll.max_rarity).unwrap_or(ItemRarity::Legendary));
            let current = record(s, item);
            let once = roll.flags & AffixRoll::ONCE != 0;
            reroll(
                &current,
                &pool(state_stats(s)),
                category,
                rules,
                roll.seed,
                once,
            )
            .ok_or(Status::REFUSED)
        });
        let next = match next {
            Ok(next) => next,
            Err(status) => return status,
        };
        let rolled = u32::try_from(next.affixes.len()).unwrap_or(0);
        let Ok(bytes) = postcard::to_allocvec(&next) else {
            return Status::REFUSED;
        };
        let stored = with(None, |s| {
            let at = s
                .content
                .item_kinds
                .iter()
                .position(|k| k == enshwick_core::stats::ITEM_KIND)
                .unwrap_or_else(|| {
                    s.content
                        .item_kinds
                        .push(enshwick_core::stats::ITEM_KIND.to_owned());
                    s.content.item_kinds.len() - 1
                });
            s.content.item_values.insert(
                (u32::try_from(at).ok()?, roll.pide.get()),
                (roll.item.get(), bytes),
            );
            Some(())
        });
        if stored.is_none() {
            return Status::MISSING;
        }
        crate::test::table::put(count, rolled)
    }

    pub(crate) fn item_lines(
        me as Caller,
        pide: enshwick_core::Entity,
        item: enshwick_core::ItemId,
        [out: *mut ItemLine, capacity: u32, count: *mut u32] as args::ListOut,
    ) -> Status {
        let lines = with(Vec::new(), |s| {
            let item = ItemInstance { pide, item };
            let record = record(s, item);
            item_lines_of(s, &record)
        });
        crate::test::table::put_list(&lines, out, capacity, count)
    }

    pub(crate) fn affix_info(
        me as Caller,
        affix: u32,
        text: *mut Str as args::Out,
        rarity: *mut u8 as args::Out,
    ) -> Status {
        let found = with(None, |s| {
            let (_, def, text) = s.inventory.stats.affixes.get(affix as usize)?;
            // The fake keeps its affixes while installed.
            Some((Str::new(text), def.rarity.to_raw()))
        });
        let Some((name, rare)) = found else {
            return Status::MISSING;
        };
        let status = crate::test::table::put(text, name);
        if status.is_ok() {
            crate::test::table::put(rarity, rare)
        } else {
            status
        }
    }
}
