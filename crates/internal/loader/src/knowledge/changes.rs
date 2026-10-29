//! Knowledge taken back, and changes for players who are not in the game.
//!
//! The game's queue only adds, so a revoke writes the value maps the way a
//! client applies what the server sends: the game's own set function puts a
//! value in or, for 0, takes the id out, and logs it. A log written after a
//! step never reaches the clients, since the next step clears the logs
//! before it sends them, so the loader asks the game for a full resend of
//! what changed, the world's to every client or one player's to that
//! player, as the game does when a client asks to recover. The save writes
//! what the maps hold, so a revoke is saved with the next save.
//!
//! A player's knowledge lives in the character's save on the player's
//! machine: the server holds it only while the player is in the game, from
//! the moment the save has loaded into the player's record. Changes for an
//! account that is not in the game wait in the world's folder,
//! `mod-data/enshwick/worlds/<key>/knowledge.toml`, and apply once the
//! account's next character has loaded in that world.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use enshwick::Entity;
use enshwick::abi::{Event, Header, KnowledgeData, KnowledgeInfo, Status};

use crate::host::Loader;
use crate::locks::Relock;
use crate::memory::write;
use crate::pinned;
use crate::pinned::{Constant, Symbol};

use super::{definition, layout, read, store};

/// The queue's file in a world's folder.
const FILE: &str = "knowledge.toml";
/// Steps between looks for queued accounts in the game.
const LOOK_EVERY: u64 = 30;

/// The game's list of values to set: ids, save states and values.
#[repr(C)]
#[derive(Debug)]
struct ValueList {
    ids: *const u32,
    count: u64,
    states: *const u8,
    state_count: u64,
    values: *const u32,
    value_count: u64,
    save: u8,
}

type SetValuesFn = unsafe extern "system" fn(map: *mut u8, log: *mut u8, list: *const ValueList);
type SetWorldFn = unsafe extern "system" fn(state: *mut u8, list: *const ValueList);

/// Where the revoke's parts lie, from the build table.
#[derive(Debug)]
struct Parts {
    set_values: SetValuesFn,
    set_world: SetWorldFn,
    record_changes: usize,
    world_touched: usize,
    replication: usize,
    connection_size: usize,
    connection_created: usize,
    connection_loaded: usize,
    resend_world: usize,
    resend_player: usize,
}

static PARTS: OnceLock<Option<Parts>> = OnceLock::new();

fn parts(loader: &Loader) -> Option<&'static Parts> {
    PARTS
        .get_or_init(|| {
            let function = |name: Symbol| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let offset = |name: Constant| loader.offset(name);
            let (set_values, set_world) = (
                function(pinned::knowledge::SET_VALUES)?,
                function(pinned::knowledge::SET_WORLD)?,
            );
            Some(Parts {
                // SAFETY: the build table's `knowledge.set_values` is the game function
                // with the `SetValuesFn` signature on this build.
                set_values: unsafe { core::mem::transmute::<usize, SetValuesFn>(set_values) },
                // SAFETY: the build table's `knowledge.set_world` is the game function
                // with the `SetWorldFn` signature on this build.
                set_world: unsafe { core::mem::transmute::<usize, SetWorldFn>(set_world) },
                record_changes: offset(pinned::knowledge::RECORD_CHANGES)?,
                world_touched: offset(pinned::knowledge::WORLD_TOUCHED)?,
                replication: offset(pinned::knowledge::REPLICATION)?,
                connection_size: offset(pinned::knowledge::CONNECTION_SIZE)?,
                connection_created: offset(pinned::knowledge::CONNECTION_CREATED)?,
                connection_loaded: offset(pinned::knowledge::CONNECTION_LOADED)?,
                resend_world: offset(pinned::knowledge::RESEND_WORLD)?,
                resend_player: offset(pinned::knowledge::RESEND_PLAYER)?,
            })
        })
        .as_ref()
}

/// The value left after taking `value` of `kind` back from `current`: a
/// bool cleared, a counter lowered, a bit cleared.
fn lowered(kind: u8, current: u32, value: u32) -> Option<u32> {
    match kind {
        KnowledgeInfo::BOOL => Some(0),
        KnowledgeInfo::COUNTER => Some(current.saturating_sub(value)),
        KnowledgeInfo::FLAGS if value < 32 => Some(current & !(1 << value)),
        _ => None,
    }
}

/// The game's knowledge replication's connection of `slot`, for the live
/// game object `game` from `store`.
fn connection(parts: &Parts, game: usize, slot: usize) -> Option<usize> {
    // SAFETY: `game` is the live game object `store` found; its knowledge
    // replication pointer sits at the build table's `knowledge.replication`
    // offset.
    let replication: usize = unsafe { read(game, parts.replication) };
    (replication != 0).then(|| replication + slot * parts.connection_size)
}

/// Whether the save of the player in record `slot` has loaded, so the
/// record holds the character's knowledge and changes to it stick.
fn loaded(parts: &Parts, game: usize, slot: usize) -> bool {
    connection(parts, game, slot).is_some_and(|conn| {
        // SAFETY: `conn` is the replication's connection of the player's
        // slot, of the build table's `knowledge.connection_size`; its u8
        // loaded mark sits at `knowledge.connection_loaded`.
        unsafe { read::<u8>(conn, parts.connection_loaded) != 0 }
    })
}

/// Whether the save of the character `entity` has loaded; `false` on a
/// build without the parts that tell.
pub(crate) fn character_loaded(loader: &Loader, entity: Entity) -> bool {
    let (Some(parts), Some(layout)) = (parts(loader), layout(loader)) else {
        return true;
    };
    let Ok((game, _)) = store(layout) else {
        return false;
    };
    (entity.0 as usize)
        .checked_sub(1)
        .is_some_and(|slot| loaded(parts, game, slot))
}

/// Changes raised after the next step, by character (0 for the world).
static RAISED: Mutex<Vec<KnowledgeData>> = Mutex::new(Vec::new());

/// Takes knowledge `id` of `kind` back from the character `entity`, or
/// from the world.
pub(crate) fn revoke(
    loader: &Loader,
    entity: u32,
    id: u32,
    kind: u32,
    value: u32,
) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let parts = parts(loader).ok_or(Status::MISSING)?;
    let (game, state) = store(layout)?;
    let (scope, def) = definition(layout, state, id).ok_or(Status::BAD_ARGS)?;
    // SAFETY: `def` is the definition `definition` found in the live blob;
    // its u8 kind sits at the reflected `type` field.
    let def_kind: u8 = unsafe { read(def, layout.def_kind) };
    if u32::from(def_kind) != kind {
        return Err(Status::BAD_ARGS);
    }
    let player = scope == KnowledgeInfo::PLAYER;
    let current = super::value(loader, if player { Entity(entity) } else { Entity(0) }, id)?.value;
    let new = lowered(def_kind, current, value).ok_or(Status::BAD_ARGS)?;
    if new == current {
        return Ok(());
    }
    let list = ValueList {
        ids: &raw const id,
        count: 1,
        states: core::ptr::null(),
        state_count: 0,
        values: &raw const new,
        value_count: 1,
        save: 0,
    };
    let records = super::records(layout, state);
    if player {
        let record = super::record(layout, state, Entity(entity))?;
        let slot = entity as usize - 1;
        if !loaded(parts, game, slot) {
            return Err(Status::REFUSED);
        }
        // SAFETY: `record` is the character's active record (`record`
        // checked it) whose save has loaded (`loaded` checked it), with its
        // log at the build table's `knowledge.record_log` offset; `store`
        // put this call on the game thread at a quiet moment; `list` and
        // what it points at live on this stack for the call.
        unsafe {
            (parts.set_values)(
                core::ptr::with_exposed_provenance_mut(record),
                core::ptr::with_exposed_provenance_mut(record + layout.record_log),
                &raw const list,
            );
        };
        // SAFETY: `record` is the character's active record, read on the game
        // thread at a quiet moment; its u16 change counter sits at the build
        // table's `knowledge.record_changes` offset.
        let counter: u16 = unsafe { read(record, parts.record_changes) };
        // SAFETY: `record` is the character's active record, written on the
        // game thread at a quiet moment, where the game writes it too; its
        // u16 change counter, which never reads 0, sits at the build table's
        // `knowledge.record_changes` offset.
        unsafe { write(record, parts.record_changes, counter.wrapping_add(1).max(1)) };
        if let Some(conn) = connection(parts, game, slot) {
            // SAFETY: `conn` is the replication's connection of the player's
            // slot, written on the game thread at a quiet moment; its u8
            // resend request sits at the build table's
            // `knowledge.resend_player` offset.
            unsafe { write(conn, parts.resend_player, 1u8) };
        }
    } else {
        // SAFETY: `state` is the live store `store` found, and `store` put
        // this call on the game thread at a quiet moment; `list` and what it
        // points at live on this stack for the call.
        unsafe {
            (parts.set_world)(
                core::ptr::with_exposed_provenance_mut(state),
                &raw const list,
            );
        };
        // SAFETY: `state` is the live store, written on the game thread at a
        // quiet moment; its u8 world-changed mark sits at the build table's
        // `knowledge.world_touched` offset.
        unsafe { write(state, parts.world_touched, 2u8) };
        for slot in 0..records {
            let Some(conn) = connection(parts, game, slot) else {
                break;
            };
            // SAFETY: `slot` is below the store's record count, so `conn` is
            // a connection of the live replication, of the build table's
            // `knowledge.connection_size`; its u8 created mark sits at
            // `knowledge.connection_created`.
            if unsafe { read::<u8>(conn, parts.connection_created) } != 0 {
                // SAFETY: `conn` is a created connection of the live
                // replication, written on the game thread at a quiet moment;
                // its u8 resend request sits at the build table's
                // `knowledge.resend_world` offset.
                unsafe { write(conn, parts.resend_world, 1u8) };
            }
        }
    }
    RAISED.relock().push(KnowledgeData {
        header: Header::of::<KnowledgeData>(),
        entity: if player { Entity(entity) } else { Entity(0) },
        id: enshwick::abi::KnowledgeId::new(id),
        value: new,
    });
    Ok(())
}

/// A change waiting for an account.
#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct Waiting {
    /// The account's key, as hex: TOML integers stop at `i64`.
    account: String,
    id: u32,
    kind: u8,
    value: u32,
    #[serde(default)]
    revoke: bool,
}

#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
#[serde(default, deny_unknown_fields)]
struct WaitingFile {
    waiting: Vec<Waiting>,
}

#[derive(Debug, Default)]
struct Queue {
    key: Option<String>,
    waiting: Vec<Waiting>,
}

static QUEUE: Mutex<Queue> = Mutex::new(Queue {
    key: None,
    waiting: Vec::new(),
});
/// Whether changes wait for accounts.
static WAITING: AtomicBool = AtomicBool::new(false);

fn file(loader: &Loader, key: &str) -> PathBuf {
    loader
        .data_dir
        .join("enshwick")
        .join("worlds")
        .join(key)
        .join(FILE)
}

/// The changes waiting in world `key`; none for a missing file, `None` for
/// one that does not read, which then stays as it is.
fn read_queue(loader: &Loader, key: &str) -> Option<Vec<Waiting>> {
    let path = file(loader, key);
    let read = enshwick::files::read_to_string_if_present(&path)
        .map_err(|e| e.to_string())
        .and_then(|text| {
            text.map(|text| toml::from_str::<WaitingFile>(&text))
                .transpose()
                .map_err(|e| e.to_string())
        });
    match read {
        Ok(file) => Some(file.map(|file| file.waiting).unwrap_or_default()),
        Err(error) => {
            crate::log!(
                WARN,
                "knowledge: {} does not read and stays as it is: {error}",
                path.display()
            );
            None
        }
    }
}

fn write_queue(loader: &Loader, key: &str, waiting: &[Waiting]) {
    let path = file(loader, key);
    let written = if waiting.is_empty() {
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
            _ => Ok(()),
        }
    } else {
        let text = format!(
            "# Knowledge changes waiting for accounts that were not in the game;\n\
             # each applies once the account's character has loaded in this world.\n{}",
            toml::to_string(&WaitingFile {
                waiting: waiting.to_vec()
            })
            .unwrap_or_default()
        );
        enshwick::files::write_atomically(&path, text)
    };
    if let Err(error) = written {
        crate::log!(WARN, "knowledge: {} not written: {error}", path.display());
    }
}

fn apply(loader: &Loader, entity: Entity, change: &Waiting) -> Result<(), Status> {
    if change.revoke {
        revoke(
            loader,
            entity.0,
            change.id,
            u32::from(change.kind),
            change.value,
        )
    } else {
        super::grant(
            loader,
            entity.0,
            change.id,
            u32::from(change.kind),
            change.value,
        )
    }
}

/// The character of the account `key` whose save has loaded, if it is in
/// the game.
fn character_of(loader: &Loader, key: u64) -> Option<Entity> {
    let players = crate::world::players(loader).ok()?;
    players
        .iter()
        .find(|p| p.key == enshwick::PlayerKey::new(key))
        .map(|p| p.entity)
        .filter(|&entity| character_loaded(loader, entity))
}

/// Applies a change of knowledge `id` for the account `key` now, when its
/// character is in the game, or queues it with the running world.
pub(crate) fn for_account(
    loader: &Loader,
    key: u64,
    id: u32,
    kind: u32,
    value: u32,
    revoking: bool,
) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    parts(loader).ok_or(Status::MISSING)?;
    let (_, state) = store(layout)?;
    let (scope, def) = definition(layout, state, id).ok_or(Status::BAD_ARGS)?;
    // SAFETY: `def` is the definition `definition` found in the live blob;
    // its u8 kind sits at the reflected `type` field.
    let def_kind: u8 = unsafe { read(def, layout.def_kind) };
    if scope != KnowledgeInfo::PLAYER || u32::from(def_kind) != kind {
        return Err(Status::BAD_ARGS);
    }
    let change = Waiting {
        account: format!("{key:016x}"),
        id,
        kind: def_kind,
        value,
        revoke: revoking,
    };
    if let Some(entity) = character_of(loader, key) {
        return apply(loader, entity, &change);
    }
    let mut queue = QUEUE.relock();
    let world = queue.key.clone().ok_or(Status::MISSING)?;
    if queue.waiting.contains(&change) {
        return Ok(());
    }
    queue.waiting.push(change);
    write_queue(loader, &world, &queue.waiting);
    WAITING.store(true, Ordering::Release);
    Ok(())
}

/// Follows the server's world: reads the queue when one loads and forgets
/// it when it shuts down.
pub(crate) fn world_event(loader: &Loader, event: Event) {
    match event {
        Event::WORLD_LOADED => {
            let mut key = crate::saves::world_key(loader).map(str::to_owned);
            let waiting = match key.as_deref().map(|key| read_queue(loader, key)) {
                Some(Some(waiting)) => waiting,
                // A world without a key queues nothing, so nothing is written.
                Some(None) => {
                    key = None;
                    Vec::new()
                }
                None => Vec::new(),
            };
            WAITING.store(!waiting.is_empty(), Ordering::Release);
            if !waiting.is_empty() {
                crate::log!(
                    INFO,
                    "knowledge: {} changes wait for accounts",
                    waiting.len()
                );
            }
            *QUEUE.relock() = Queue { key, waiting };
        }
        Event::SHUTDOWN_STARTED => {
            WAITING.store(false, Ordering::Release);
            *QUEUE.relock() = Queue::default();
            RAISED.relock().clear();
        }
        _ => {}
    }
}

/// After a step: raises the revokes of the step before and, now and then,
/// applies what waits for accounts now in the game.
pub(crate) fn after_step(loader: &Loader, step: u64) {
    let raised = std::mem::take(&mut *RAISED.relock());
    for data in &raised {
        loader.emit(Event::KNOWLEDGE_CHANGED, None, (&raw const *data).cast());
    }
    if WAITING.load(Ordering::Acquire) && step.is_multiple_of(LOOK_EVERY) {
        settle(loader);
    }
}

#[cold]
fn settle(loader: &Loader) {
    let Ok(players) = crate::world::players(loader) else {
        return;
    };
    let mut queue = QUEUE.relock();
    let before = queue.waiting.len();
    queue.waiting.retain(|change| {
        let Ok(key) = u64::from_str_radix(&change.account, 16) else {
            return false;
        };
        let Some(player) = players
            .iter()
            .find(|p| p.key == enshwick::PlayerKey::new(key))
        else {
            return true;
        };
        if !character_loaded(loader, player.entity) {
            return true;
        }
        match apply(loader, player.entity, change) {
            Ok(()) => false,
            Err(status) if status == Status::REFUSED => true,
            Err(status) => {
                crate::log!(
                    WARN,
                    "knowledge: change of id {} for an account dropped ({})",
                    change.id,
                    status.0
                );
                false
            }
        }
    });
    if queue.waiting.len() != before {
        crate::log!(
            INFO,
            "knowledge: {} waiting changes applied, {} left",
            before - queue.waiting.len(),
            queue.waiting.len()
        );
        if let Some(world) = queue.key.clone() {
            write_queue(loader, &world, &queue.waiting);
        }
        WAITING.store(!queue.waiting.is_empty(), Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_revoke_clears_lowers_or_unsets() {
        assert_eq!(lowered(KnowledgeInfo::BOOL, 1, 0), Some(0));
        assert_eq!(lowered(KnowledgeInfo::COUNTER, 5, 2), Some(3));
        assert_eq!(lowered(KnowledgeInfo::COUNTER, 5, u32::MAX), Some(0));
        assert_eq!(lowered(KnowledgeInfo::FLAGS, 0b111, 1), Some(0b101));
        assert_eq!(lowered(KnowledgeInfo::FLAGS, 1, 32), None);
        assert_eq!(lowered(9, 1, 0), None);
    }

    #[test]
    fn the_queue_file_round_trips() {
        let file = WaitingFile {
            waiting: vec![Waiting {
                account: format!("{:016x}", u64::MAX),
                id: 7,
                kind: KnowledgeInfo::FLAGS,
                value: 1,
                revoke: true,
            }],
        };
        let text = toml::to_string(&file).unwrap_or_default();
        let back: WaitingFile = toml::from_str(&text).unwrap_or_default();
        assert_eq!(back.waiting, file.waiting);
        assert_eq!(
            u64::from_str_radix(&back.waiting[0].account, 16).ok(),
            Some(u64::MAX)
        );
    }

    #[test]
    fn the_value_list_is_the_games_size() {
        assert_eq!(size_of::<ValueList>(), 0x38);
    }
}
