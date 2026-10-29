//! Player deaths with their cause.
//!
//! When a player's health reaches 0, the system that took it, for a hit, a
//! fall, the shroud or drowning, hands the game's tracking a `PlayerDeath`
//! record: the account, the cause, the place, the killer's template for a
//! hit, whether the shroud was deadly and whether the player drowned. The
//! loader detours that sink, which runs on the systems' worker threads,
//! copies the record and raises [`Event::PLAYER_DIED`] after the step, with
//! the attacker of the last hit in the player's `HitHistory` for a hit.
//! Deaths no system records, such as freezing or an administrator's kill,
//! raise nothing here.

use std::sync::{Mutex, OnceLock};

use enshwick::Entity;
use enshwick::abi::{Event, Header, HookFlags, PlayerDeathData};

use crate::chains::Next;
use crate::host::{Loader, Target};
use crate::locks::Relock;
use crate::pinned;

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[pinned::player::DEATH_SINK.id()];

/// The most deaths kept between two steps.
const MOST: usize = 64;

type SinkFn =
    unsafe extern "system" fn(out: *mut u8, tracker: *mut u8, death: *const u8) -> *mut u8;

// SAFETY: `SinkFn` is the exact signature of the game's death sink at the
// table's `player.death_sink`, and the slot is used only as the `next` of the
// `sink` link on that function.
static SINK_NEXT: Next<SinkFn> = unsafe { Next::new() };

/// Where the record keeps what the event carries, from reflection.
#[derive(Debug, Clone, Copy)]
struct Record {
    size: usize,
    account: usize,
    cause: usize,
    position: usize,
    cause_guid: usize,
    was_deadly_fog: usize,
    drowned: usize,
}

/// Where a `HitHistory` keeps the attacker of its last hit, from reflection.
#[derive(Debug, Clone, Copy)]
struct History {
    hash: u32,
    last_index: usize,
    entries: usize,
    slots: usize,
    entry_size: usize,
    attacker: usize,
}

impl History {
    fn resolve(loader: &Loader) -> Option<Self> {
        let r = &loader.reflection;
        let offset = |ty: &str, name: &str| {
            r.field_offset(ty, name)
                .ok()
                .and_then(|v| usize::try_from(v).ok())
        };
        let size = |ty: &str| {
            r.type_info(ty)
                .ok()
                .and_then(|info| usize::try_from(info.size()).ok())
        };
        let ty = "keen::ecs::HitHistory";
        let entries = offset(ty, "history")?;
        let entry_size = size("keen::ecs::HitEvent")?.max(1);
        // The ring of hits ends where `targetTransform` starts.
        let slots = offset(ty, "targetTransform")?.saturating_sub(entries) / entry_size;
        let history = Self {
            hash: crate::world::type_hash(r.type_info(ty).ok()?.va())?,
            last_index: offset(ty, "lastIndex")?,
            entries,
            slots,
            entry_size,
            attacker: offset("keen::ecs::BaseHitEvent", "rootSourceId")?,
        };
        let id = size_of::<u32>();
        (history.last_index + id <= size(ty)?
            && history.entries + history.slots * history.entry_size <= size(ty)?
            && history.attacker + id <= history.entry_size)
            .then_some(history)
    }

    /// The attacker of the last hit `entity` took; 0 for none.
    fn killer(&self, loader: &Loader, entity: Entity) -> u32 {
        let Ok(history) = crate::world::component(loader, entity, self.hash) else {
            return 0;
        };
        let history = history.expose_provenance();
        // SAFETY: `world::component` returned the entity's live HitHistory,
        // read on the game thread after the step; `resolve` checked that the
        // `u32` at `lastIndex` lies inside the reflected size.
        let last: u32 = unsafe { crate::memory::read(history, self.last_index) };
        let Some(last) = usize::try_from(last).ok().filter(|&i| i < self.slots) else {
            return 0;
        };
        let at = self.entries + last * self.entry_size + self.attacker;
        // SAFETY: the live HitHistory from `world::component`, on the game
        // thread after the step; `last` is below `slots` by the earlier filter,
        // and `resolve` checked that every slot, with the `u32` attacker
        // inside it, lies inside the reflected size.
        unsafe { crate::memory::read(history, at) }
    }
}

static RECORD: OnceLock<Record> = OnceLock::new();
static HISTORY: OnceLock<Option<History>> = OnceLock::new();
/// Deaths seen since the last step, as the records' bytes.
static SEEN: Mutex<Vec<[u8; 64]>> = Mutex::new(Vec::new());

fn record(loader: &Loader) -> Option<Record> {
    let ty = "keen::PlayerDeath";
    let field = |name: &str| {
        loader
            .reflection
            .field_offset(ty, name)
            .ok()
            .and_then(|v| usize::try_from(v).ok())
    };
    let size = loader
        .reflection
        .type_info(ty)
        .ok()
        .and_then(|info| usize::try_from(info.size()).ok())
        .filter(|&size| size <= 64)?;
    Some(Record {
        size,
        account: field("accountIdHash")?,
        cause: field("cause")?,
        position: field("position")?,
        cause_guid: field("causeGuid")?,
        was_deadly_fog: field("wasDeadlyFog")?,
        drowned: field("drowned")?,
    })
}

/// Links the sink when a mod listens for deaths.
pub(crate) fn link(loader: &'static Loader) {
    if !loader.subscribed(Event::PLAYER_DIED) {
        return;
    }
    let Some(layout) = record(loader) else {
        crate::log!(WARN, "player deaths off: no death record in this build");
        return;
    };
    let _first = RECORD.set(layout);
    let history = HISTORY.get_or_init(|| History::resolve(loader));
    if history.is_none() {
        crate::log!(
            WARN,
            "player deaths: no hit history in this build, killers stay 0"
        );
    }
    loader.link_feature(
        "player deaths",
        Target::Pinned(pinned::player::DEATH_SINK),
        &SINK_NEXT,
        sink,
        HookFlags::NONE,
    );
}

/// The switch of the player deaths.
static DEATHS_FUSE: crate::fuse::Fuse = crate::fuse::Fuse::new("player deaths");

unsafe extern "system" fn sink(out: *mut u8, tracker: *mut u8, death: *const u8) -> *mut u8 {
    if let Some(layout) = RECORD.get()
        && !death.is_null()
    {
        let _caught = DEATHS_FUSE.run("the sink", || {
            let mut bytes = [0u8; 64];
            for (at, byte) in bytes.iter_mut().enumerate().take(layout.size) {
                // SAFETY: the game hands the sink a non-null record (checked
                // earlier) of the reflected `PlayerDeath` size, valid for the
                // call, and `at` is below that size.
                *byte = unsafe { enshwick::raw::read_at(death, at) };
            }
            let mut seen = SEEN.relock();
            if seen.len() < MOST {
                seen.push(bytes);
            }
        });
    }
    // SAFETY: the game's own arguments, passed on unchanged to the rest of
    // the chain, which has the death sink's signature.
    unsafe { SINK_NEXT.get()(out, tracker, death) }
}

fn field<const N: usize>(bytes: &[u8; 64], at: usize) -> [u8; N] {
    bytes
        .get(at..at + N)
        .and_then(|b| b.try_into().ok())
        .unwrap_or([0; N])
}

/// The event data of one record, with the character of the account when it
/// is in the game.
fn data(layout: &Record, bytes: &[u8; 64], entity_of: impl Fn(u64) -> u32) -> PlayerDeathData {
    let key = u64::from_le_bytes(field(bytes, layout.account));
    let position: [u8; 12] = field(bytes, layout.position);
    let coordinate = |i: usize| {
        f32::from_le_bytes(
            position
                .get(i * 4..i * 4 + 4)
                .and_then(|b| b.try_into().ok())
                .unwrap_or([0; 4]),
        )
    };
    PlayerDeathData {
        header: Header::of::<PlayerDeathData>(),
        entity: Entity(entity_of(key)),
        cause: u32::from(field::<1>(bytes, layout.cause)[0]),
        key: enshwick::PlayerKey::new(key),
        position: [coordinate(0), coordinate(1), coordinate(2)],
        cause_guid: field(bytes, layout.cause_guid),
        was_deadly_fog: field::<1>(bytes, layout.was_deadly_fog)[0],
        drowned: field::<1>(bytes, layout.drowned)[0],
        reserved: 0,
        killer_entity: Entity(0),
        reserved_tail: 0,
    }
}

/// Raises the deaths of the last step, on the game thread after it.
pub(crate) fn after_step(loader: &Loader) {
    let Some(layout) = RECORD.get() else {
        return;
    };
    let seen = std::mem::take(&mut *SEEN.relock());
    if seen.is_empty() {
        return;
    }
    let players = crate::world::players(loader).unwrap_or_default();
    let history = HISTORY.get().copied().flatten();
    for bytes in &seen {
        let mut death = data(layout, bytes, |key| {
            players
                .iter()
                .find(|p| p.key == enshwick::PlayerKey::new(key))
                .map_or(0, |p| p.entity.0)
        });
        if death.cause == PlayerDeathData::IMPACT
            && death.entity != Entity(0)
            && let Some(history) = history
        {
            death.killer_entity = Entity(history.killer(loader, death.entity));
        }
        loader.emit(Event::PLAYER_DIED, None, (&raw const death).cast());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_record_reads_into_the_event() {
        let layout = Record {
            size: 48,
            account: 0,
            cause: 12,
            position: 16,
            cause_guid: 28,
            was_deadly_fog: 44,
            drowned: 45,
        };
        let mut bytes = [0u8; 64];
        bytes[..8].copy_from_slice(&7u64.to_le_bytes());
        bytes[12] = 3;
        bytes[16..20].copy_from_slice(&1.5f32.to_le_bytes());
        bytes[28] = 0xab;
        bytes[45] = 1;
        let death = data(&layout, &bytes, |key| if key == 7 { 2 } else { 0 });
        assert_eq!(death.entity, Entity(2));
        assert_eq!(death.key, enshwick::PlayerKey::new(7));
        assert_eq!(death.cause, PlayerDeathData::SUFFOCATED);
        assert!((death.position[0] - 1.5).abs() < f32::EPSILON);
        assert_eq!(death.cause_guid[0], 0xab);
        assert_eq!((death.was_deadly_fog, death.drowned), (0, 1));
    }
}
