//! The fake loader's stats: kinds, modifiers on entities and buffs, the
//! items each entity wears, and values worked out on every read.

use core::ffi::c_void;
use std::collections::{BTreeMap, BTreeSet};

use crate::abi::{
    Header, ModHandle, StatChange, StatChangeFn, StatDefData, StatModifierData, StatModify,
    StatRef, StatSource, Status, Str,
};
use crate::item_data::ItemInstance;
use crate::stats::{ItemStats, Op, StatDef, aggregate, builtin, key_of};

use super::table::with;

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
    buffs: Vec<(u32, StatModifierData)>,
    handlers: Vec<(StatChangeFn, usize)>,
    watched: BTreeSet<u32>,
    /// What each entity wears.
    pub(crate) worn: BTreeMap<u32, Vec<ItemInstance>>,
    /// The values the last settle raised, by entity and stat.
    settled: BTreeMap<(u32, u32), f32>,
}

impl FakeStats {
    fn with_builtins() -> Self {
        let mut stats = Self::default();
        for (id, def) in builtin::ALL {
            let def = def.to_def();
            let text = def.text.clone().into_boxed_str();
            stats.kinds.push(Kind {
                id: id.into(),
                def: Some((def, text)),
            });
        }
        stats
    }
}

fn full(id: &str) -> Option<String> {
    let part = |p: &str| {
        (1..=64).contains(&p.len())
            && p.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
    };
    let (namespace, name) = id.split_once(':').unwrap_or(("fake", id));
    (part(namespace) && part(name)).then(|| format!("{namespace}:{name}"))
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

fn state_stats(s: &mut super::table::State) -> &mut FakeStats {
    if s.stats.kinds.is_empty() {
        s.stats = FakeStats::with_builtins();
    }
    &mut s.stats
}

pub(crate) extern "system" fn stat_open(
    _me: ModHandle,
    id: Str,
    def: *const StatDefData,
    out: *mut StatRef,
) -> Status {
    // SAFETY: strings are borrowed for the call.
    let Some(id) = unsafe { id.as_str() }.and_then(full) else {
        return Status::BAD_ARGS;
    };
    // SAFETY: the safe layer passes a valid definition or null.
    let def = unsafe { def.as_ref() };
    let parsed = match def {
        // SAFETY: the definition's text is borrowed for the call.
        Some(data) => match unsafe { StatDef::from_data(data) } {
            Some(def) => Some(def),
            None => return Status::BAD_ARGS,
        },
        None => None,
    };
    let opened = with(Err(Status::MISSING), |s| {
        let stats = state_stats(s);
        let index = index_of(stats, &id);
        if let Some(def) = parsed {
            let kind = &mut stats.kinds[index as usize];
            if kind.def.is_some() || !id.starts_with("fake:") {
                return Err(Status::REFUSED);
            }
            let text = def.text.clone().into_boxed_str();
            kind.def = Some((def, text));
        }
        Ok(index)
    });
    match opened {
        Ok(index) if !out.is_null() => {
            // SAFETY: checked above; the safe layer's out pointer.
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

pub(crate) extern "system" fn stat_info(
    _me: ModHandle,
    stat: u32,
    def: *mut StatDefData,
    id: *mut Str,
) -> Status {
    let found = with(None, |s| {
        let kind = state_stats(s).kinds.get(stat as usize)?;
        let (def, text) = kind.def.as_ref()?;
        let mut data = def.to_data();
        // The fake keeps the text and id while installed, as the loader
        // keeps them for the process.
        data.text = Str::new(text);
        Some((data, Str::new(&kind.id)))
    });
    let Some((data, name)) = found else {
        return Status::MISSING;
    };
    if def.is_null() || id.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: checked above; the safe layer's out pointers.
    unsafe { def.write(data) };
    // SAFETY: as above.
    unsafe { id.write(name) };
    Status::OK
}

/// Every modifier toward `stat` (all for `u32::MAX`) on `entity`, in
/// source order.
fn sources(s: &mut super::table::State, entity: u32, stat: u32) -> Vec<StatSource> {
    let worn = state_stats(s)
        .worn
        .get(&entity)
        .cloned()
        .unwrap_or_default();
    let kind = s
        .item_kinds
        .iter()
        .position(|k| k == crate::stats::ITEM_KIND);
    let mut out = Vec::new();
    let mut items = worn;
    items.sort_unstable();
    for item in items {
        let record = kind
            .and_then(|kind| s.item_values.get(&(u32::try_from(kind).ok()?, item.pide.0)))
            .filter(|(held, _)| *held == item.item)
            .and_then(|(_, bytes)| postcard::from_bytes::<ItemStats>(bytes).ok())
            .unwrap_or_default();
        for m in record.modifiers {
            let Some(index) = s
                .stats
                .kinds
                .iter()
                .position(|k| key_of(&k.id) == m.stat)
                .and_then(|i| u32::try_from(i).ok())
            else {
                continue;
            };
            out.push(StatSource {
                kind: StatSource::ITEM,
                op: m.op.to_raw(),
                reserved: 0,
                stat: index,
                value: m.value,
                id: item.pide.0,
                owner: item.item,
            });
        }
    }
    let mut buffs: Vec<u32> = s
        .buffs
        .get(&entity)
        .map(|on| on.iter().map(|b| b.id).collect())
        .unwrap_or_default();
    buffs.sort_unstable();
    for buff in buffs {
        for (_, m) in s.stats.buffs.iter().filter(|(id, _)| *id == buff) {
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

fn value(s: &mut super::table::State, entity: u32, stat: u32) -> Option<f32> {
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

pub(crate) extern "system" fn stat_value(
    _me: ModHandle,
    entity: u32,
    stat: u32,
    out: *mut f32,
) -> Status {
    match with(None, |s| value(s, entity, stat)) {
        Some(v) if !out.is_null() => {
            // SAFETY: checked above; the safe layer's out pointer.
            unsafe { out.write(v) };
            Status::OK
        }
        Some(_) => Status::BAD_ARGS,
        None => Status::MISSING,
    }
}

pub(crate) extern "system" fn stat_modify(_me: ModHandle, request: *const StatModify) -> Status {
    // SAFETY: the safe layer passes a valid request.
    let Some(request) = (unsafe { request.as_ref() }) else {
        return Status::BAD_ARGS;
    };
    with(Status::MISSING, |s| {
        let saved = request.flags & StatModify::SAVED != 0;
        if saved && !s.host.players.iter().any(|p| p.entity == request.entity) {
            return Status::REFUSED;
        }
        let stats = state_stats(s);
        let at = (request.entity, request.key, request.modifier.stat);
        if request.flags & StatModify::REMOVE != 0 {
            stats.modifiers.remove(&at);
        } else {
            let m = request.modifier;
            stats
                .modifiers
                .insert(at, (m.op, m.value, request.flags, request.duration_ms));
        }
        stats.watched.insert(request.entity);
        Status::OK
    })
}

pub(crate) extern "system" fn stat_buff(
    _me: ModHandle,
    buff: u32,
    modifiers: *const StatModifierData,
    count: u32,
) -> Status {
    if modifiers.is_null() && count != 0 {
        return Status::BAD_ARGS;
    }
    let list = if count == 0 {
        Vec::new()
    } else {
        // SAFETY: the safe layer passes `count` modifiers.
        unsafe { core::slice::from_raw_parts(modifiers, count as usize) }.to_vec()
    };
    with(Status::MISSING, |s| {
        state_stats(s)
            .buffs
            .extend(list.into_iter().map(|m| (buff, m)));
        Status::OK
    })
}

pub(crate) extern "system" fn stats_changed(
    _me: ModHandle,
    callback: StatChangeFn,
    user: *mut c_void,
) -> Status {
    with(Status::MISSING, |s| {
        state_stats(s)
            .handlers
            .push((callback, user.expose_provenance()));
        Status::OK
    })
}

pub(crate) extern "system" fn stat_sources(
    _me: ModHandle,
    entity: u32,
    stat: u32,
    out: *mut StatSource,
    capacity: u32,
    count: *mut u32,
) -> Status {
    let list = with(Vec::new(), |s| sources(s, entity, stat));
    super::table::put_list(&list, out, capacity, count)
}

pub(crate) extern "system" fn stat_watch(_me: ModHandle, entity: u32, watch: u8) -> Status {
    with(Status::MISSING, |s| {
        let stats = state_stats(s);
        if watch == 0 {
            stats.watched.remove(&entity);
        } else {
            stats.watched.insert(entity);
        }
        Status::OK
    })
}

/// Works out every watched entity's values and raises the handlers for each
/// that changed since the last settle, as the loader does after a step;
/// how many changes it raised.
pub(crate) fn settle() -> usize {
    let (changes, handlers) = with((Vec::new(), Vec::new()), |s| {
        let stats = state_stats(s);
        let mut entities: BTreeSet<u32> = stats.watched.clone();
        entities.extend(stats.worn.keys());
        entities.extend(s.host.players.iter().map(|p| p.entity));
        let kinds = u32::try_from(s.stats.kinds.len()).unwrap_or(0);
        let mut changes = Vec::new();
        for entity in entities {
            for stat in 0..kinds {
                let Some(new) = value(s, entity, stat) else {
                    continue;
                };
                let default = s.stats.kinds[stat as usize]
                    .def
                    .as_ref()
                    .map_or(0.0, |(d, _)| d.default);
                let old = s
                    .stats
                    .settled
                    .get(&(entity, stat))
                    .copied()
                    .unwrap_or(default);
                if old.to_bits() != new.to_bits() {
                    s.stats.settled.insert((entity, stat), new);
                    let key = key_of(&s.stats.kinds[stat as usize].id).0;
                    changes.push(StatChange {
                        header: Header::of::<StatChange>(),
                        entity,
                        stat: StatRef { index: stat, key },
                        old,
                        new,
                        reserved: 0,
                    });
                }
            }
        }
        (changes, s.stats.handlers.clone())
    });
    for change in &changes {
        for &(callback, user) in &handlers {
            // SAFETY: a valid change; `user` is the mod's pointer.
            unsafe { callback(change, core::ptr::with_exposed_provenance_mut(user)) };
        }
    }
    changes.len()
}

/// The modifiers the mod keeps: `(entity, key hash, stat index, op, value,
/// flags, duration in ms)`.
pub(crate) fn modifiers() -> Vec<(u32, u32, u32, u8, f32, u32, u64)> {
    with(Vec::new(), |s| {
        s.stats
            .modifiers
            .iter()
            .map(|(&(e, k, st), &(op, v, f, d))| (e, k, st, op, v, f, d))
            .collect()
    })
}
