//! Knowledge: the values the game keeps for the world and for each player,
//! which recipes, quests, journal pages, skill points and much more read;
//! reads, grants, and the changes and completed quests each step brings.
//!
//! The game keeps knowledge outside the ECS, in its knowledge store: one
//! value map for the world, one record per player slot with a value map
//! each, the record of a player's character at `entity - 1`. Every id is
//! defined in `GameKnowledgeResource`, in the world's map or the players',
//! with its kind (bool, counter, flags) and the flag bits it expects.
//!
//! A grant goes into the game's own knowledge trigger queue, the one its
//! systems push into from worker threads; `update_knowledge_system_server`
//! applies it in the next step with everything that follows: experience,
//! unlocks chained on it, the knowledge channel to the clients, the save.
//! The game checks nothing of what is queued, so the loader checks the id,
//! its kind and its bits first. Each update logs what changed for the world
//! and each player; the logs live until the next update, so the loader
//! reads them after the step for the change events. A quest's id is a
//! player's bool the game sets when it rewards the quest.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};

use enshwick::abi::{Event, Header, KnowledgeData, KnowledgeInfo, QuestData, Status};
use enshwick::ecs::Entity;
use rustc_hash::FxHashSet;

use crate::host::Loader;
use crate::memory::read;
use crate::pinned;
use crate::pinned::{Constant, Symbol};

/// The build table ids this part of the loader names.
pub(crate) const TABLE_IDS: &[&str] = &[
    pinned::knowledge::CONNECTION_CREATED.id(),
    pinned::knowledge::CONNECTION_LOADED.id(),
    pinned::knowledge::CONNECTION_SIZE.id(),
    pinned::knowledge::DEFS.id(),
    pinned::knowledge::DEFS_BLOB.id(),
    pinned::knowledge::INPUT_ACTION.id(),
    pinned::knowledge::INPUT_CHECK.id(),
    pinned::knowledge::JOURNAL.id(),
    pinned::knowledge::JOURNAL_BLOB.id(),
    pinned::knowledge::JOURNAL_BLOB_GET.id(),
    pinned::knowledge::LOG_ENTRY_SIZE.id(),
    pinned::knowledge::OBJECT.id(),
    pinned::knowledge::PLAYER_VALUE.id(),
    pinned::knowledge::PUSH.id(),
    pinned::knowledge::QUEUE.id(),
    pinned::knowledge::RECORD_ACTIVE.id(),
    pinned::knowledge::RECORD_CHANGES.id(),
    pinned::knowledge::RECORD_COUNT.id(),
    pinned::knowledge::RECORD_LOG.id(),
    pinned::knowledge::RECORD_LOG_COUNT.id(),
    pinned::knowledge::RECORD_SIZE.id(),
    pinned::knowledge::RECORDS.id(),
    pinned::knowledge::REPLICATION.id(),
    pinned::knowledge::REQUEST_KIND.id(),
    pinned::knowledge::REQUEST_TARGET.id(),
    pinned::knowledge::REQUEST_VALUE.id(),
    pinned::knowledge::RESEND_PLAYER.id(),
    pinned::knowledge::RESEND_WORLD.id(),
    pinned::knowledge::SET_VALUES.id(),
    pinned::knowledge::SET_WORLD.id(),
    pinned::knowledge::STATE.id(),
    pinned::knowledge::STEP.id(),
    pinned::knowledge::WORLD_LOG.id(),
    pinned::knowledge::WORLD_LOG_COUNT.id(),
    pinned::knowledge::WORLD_TOUCHED.id(),
    pinned::knowledge::WORLD_VALUE.id(),
    pinned::server::PLAYER_INFOS.id(),
];

pub(crate) mod changes;
pub(crate) mod guard;

type PlayerValueFn =
    unsafe extern "system" fn(state: *const u8, id: *const u32, entity: u32) -> u64;
type WorldValueFn = unsafe extern "system" fn(state: *const u8, id: *const u32) -> u64;
type PushFn = unsafe extern "system" fn(queue: *mut u8, request: *const [u8; 16]) -> u8;

/// More player records than a server keeps: a count past it is a misread.
const MAX_RECORDS: u64 = 4096;
/// More changes than one update logs: a count past it is a misread.
const MAX_LOG: u32 = 1 << 16;
/// The bytes of a trigger request, which the game's push copies whole.
const REQUEST: usize = 16;

/// Where the store and its parts lie, from the build table and reflection.
#[derive(Debug)]
struct Layout {
    player_value: PlayerValueFn,
    world_value: WorldValueFn,
    push: PushFn,
    game: usize,
    object: usize,
    state: usize,
    defs: usize,
    defs_blob: usize,
    queue: usize,
    records: usize,
    record_count: usize,
    record_size: usize,
    record_active: usize,
    step: usize,
    world_log: usize,
    world_log_count: usize,
    record_log: usize,
    record_log_count: usize,
    log_entry: usize,
    /// A trigger request's definition kind, value and target, after its id.
    request_kind: usize,
    request_value: usize,
    request_target: usize,
    journal: usize,
    journal_blob: usize,
    world_map: usize,
    player_map: usize,
    world_def: usize,
    player_def: usize,
    def_kind: usize,
    def_flags: usize,
    quests: usize,
    quest_size: usize,
    quest_id: usize,
}

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();
/// The quest ids, read once from the journal.
static QUESTS: OnceLock<FxHashSet<u32>> = OnceLock::new();
/// The knowledge update last seen after a step.
static SEEN_STEP: AtomicU32 = AtomicU32::new(0);
/// Set while nobody handles the changes: [`SEEN_STEP`] is stale.
static UNSEEN: AtomicBool = AtomicBool::new(false);

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let reflection = &loader.reflection;
            let field = |ty: &str, name: &str| {
                reflection
                    .field_offset(ty, name)
                    .ok()
                    .and_then(|v| usize::try_from(v).ok())
            };
            let size = |ty: &str| {
                reflection
                    .type_info(ty)
                    .ok()
                    .and_then(|info| usize::try_from(info.size()).ok())
            };
            let function = |name: Symbol| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let offset = |name: Constant| loader.offset(name);
            let (player_value, world_value, push) = (
                function(pinned::knowledge::PLAYER_VALUE)?,
                function(pinned::knowledge::WORLD_VALUE)?,
                function(pinned::knowledge::PUSH)?,
            );
            let defs = "keen::GameKnowledgeResource";
            let def = "keen::KnowledgeObjectResource";
            Some(Layout {
                // SAFETY: the build table's `knowledge.player_value` is the game function
                // with the `PlayerValueFn` signature on this build.
                player_value: unsafe { core::mem::transmute::<usize, PlayerValueFn>(player_value) },
                // SAFETY: the build table's `knowledge.world_value` is the game function
                // with the `WorldValueFn` signature on this build.
                world_value: unsafe { core::mem::transmute::<usize, WorldValueFn>(world_value) },
                // SAFETY: the build table's `knowledge.push` is the game function
                // with the `PushFn` signature on this build.
                push: unsafe { core::mem::transmute::<usize, PushFn>(push) },
                game: offset(pinned::server::PLAYER_INFOS)?,
                object: offset(pinned::knowledge::OBJECT)?,
                state: offset(pinned::knowledge::STATE)?,
                defs: offset(pinned::knowledge::DEFS)?,
                defs_blob: offset(pinned::knowledge::DEFS_BLOB)?,
                queue: offset(pinned::knowledge::QUEUE)?,
                records: offset(pinned::knowledge::RECORDS)?,
                record_count: offset(pinned::knowledge::RECORD_COUNT)?,
                record_size: offset(pinned::knowledge::RECORD_SIZE)?,
                record_active: offset(pinned::knowledge::RECORD_ACTIVE)?,
                step: offset(pinned::knowledge::STEP)?,
                world_log: offset(pinned::knowledge::WORLD_LOG)?,
                world_log_count: offset(pinned::knowledge::WORLD_LOG_COUNT)?,
                record_log: offset(pinned::knowledge::RECORD_LOG)?,
                record_log_count: offset(pinned::knowledge::RECORD_LOG_COUNT)?,
                log_entry: offset(pinned::knowledge::LOG_ENTRY_SIZE)?,
                request_kind: offset(pinned::knowledge::REQUEST_KIND)?,
                request_value: offset(pinned::knowledge::REQUEST_VALUE)?,
                request_target: offset(pinned::knowledge::REQUEST_TARGET)?,
                journal: offset(pinned::knowledge::JOURNAL)?,
                journal_blob: offset(pinned::knowledge::JOURNAL_BLOB)?,
                world_map: field(defs, "worldKnowledge")?,
                player_map: field(defs, "playerKnowledge")?,
                world_def: size("keen::WorldKnowledgeObjectResource")?,
                player_def: size("keen::PlayerKnowledgeObjectResource")?,
                def_kind: field(def, "type")?,
                def_flags: field(def, "expectedFlags")?,
                quests: field("keen::JournalRegistryResource", "quests")?,
                quest_size: size("keen::JournalQuestResource")?,
                quest_id: field("keen::JournalCollectionResource", "entryId")?,
            })
        })
        .as_ref()
}

/// A blob array at `header`: where its elements start and how many.
///
/// # Safety
///
/// - `header` is a live blob array header, readable for 8 bytes: an `i32`
///   offset from itself, then a `u32` count.
/// - Nothing writes the header during the call.
unsafe fn blob_array(header: usize) -> (usize, usize) {
    // SAFETY: the caller guarantees 8 readable bytes of a live header,
    // starting with its i32 offset.
    let offset: i32 = unsafe { read(header, 0) };
    // SAFETY: the caller guarantees 8 readable bytes of a live header, with
    // its u32 count at 4.
    let count: u32 = unsafe { read(header, 4) };
    let start = if offset == 0 {
        0
    } else {
        header.wrapping_add_signed(offset as isize)
    };
    (start, count as usize)
}

/// The game object and its knowledge store, on the game thread.
fn store(layout: &Layout) -> Result<(usize, usize), Status> {
    store_of(layout, crate::world::server()?)
}

/// The game object and its knowledge store of `server`, the running server
/// from [`crate::world::server`] or [`crate::world::SERVER`].
fn store_of(layout: &Layout, server: usize) -> Result<(usize, usize), Status> {
    // SAFETY: callers pass the running server, non-zero and live while its
    // world runs; its game object pointer sits at the build table's
    // `server.player_infos` offset.
    let game: usize = unsafe { read(server, layout.game) };
    if game == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `game` is the server's non-null game object by the check
    // before it; its knowledge object pointer sits at the build table's
    // `knowledge.object` offset.
    let object: usize = unsafe { read(game, layout.object) };
    if object == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `object` is the game's non-null knowledge object by the check
    // before it; its store pointer sits at the build table's `knowledge.state`
    // offset.
    let state: usize = unsafe { read(object, layout.state) };
    if state == 0 {
        return Err(Status::MISSING);
    }
    Ok((game, state))
}

/// The definition of `id` in the live store `state` from [`store_of`]: its
/// scope and its data, or `None` for an id the game does not know.
fn definition(layout: &Layout, state: usize, id: u32) -> Option<(u8, usize)> {
    // SAFETY: `state` is the live store `store_of` found; its definitions'
    // holder pointer sits at the build table's `knowledge.defs` offset.
    let holder: usize = unsafe { read(state, layout.defs) };
    if holder == 0 {
        return None;
    }
    // SAFETY: `holder` is the store's non-null definitions' holder by the
    // check before it; its blob pointer sits at the build table's
    // `knowledge.defs_blob` offset.
    let blob: usize = unsafe { read(holder, layout.defs_blob) };
    if blob == 0 {
        return None;
    }
    let world = lookup(blob + layout.world_map, id, layout.world_def);
    world.map(|def| (KnowledgeInfo::WORLD, def)).or_else(|| {
        lookup(blob + layout.player_map, id, layout.player_def)
            .map(|def| (KnowledgeInfo::PLAYER, def))
    })
}

/// Finds `id` in a static hash map of the blob, its elements `stride`
/// bytes: buckets of `{start, count}`, keys and values as blob arrays.
fn lookup(map: usize, id: u32, stride: usize) -> Option<usize> {
    // SAFETY: `map` is a reflected map field of the live definitions' blob
    // (`definition`'s callers), and a static map starts with the blob array
    // header of its buckets.
    let (buckets, bucket_count) = unsafe { blob_array(map) };
    if bucket_count == 0 || buckets == 0 {
        return None;
    }
    let bucket = id as usize & (bucket_count - 1);
    // SAFETY: `bucket` is `id` masked by the non-zero bucket count less one,
    // the game's own bucket pick, so its 8-byte `{start, count}` lies inside
    // the bucket array of a power-of-two count.
    let start: u32 = unsafe { read(buckets, bucket * 8) };
    // SAFETY: `bucket` is `id` masked by the non-zero bucket count less one,
    // the game's own bucket pick, so its u32 count at 4 lies inside the
    // bucket array of a power-of-two count.
    let count: u32 = unsafe { read(buckets, bucket * 8 + 4) };
    // SAFETY: the map's key array header follows its bucket array header,
    // 8 bytes into the live map.
    let (keys, _) = unsafe { blob_array(map + 8) };
    // SAFETY: the map's value array header follows its key array header,
    // 16 bytes into the live map.
    let (values, _) = unsafe { blob_array(map + 16) };
    (start..start.saturating_add(count))
        .map(|i| i as usize)
        // SAFETY: the bucket's `{start, count}` names keys of the map's key
        // array, each a u32.
        .find(|&i| unsafe { read::<u32>(keys, i * 4) } == id)
        .map(|i| values + i * stride)
}

/// How many player records the store holds.
fn records(layout: &Layout, state: usize) -> usize {
    // SAFETY: `state` is the live store `store_of` found; its u64 record
    // count sits at the build table's `knowledge.record_count` offset.
    let count: u64 = unsafe { read(state, layout.record_count) };
    usize::try_from(count).unwrap_or(0)
}

/// The player record of the character `entity`, active, or why not.
fn record(layout: &Layout, state: usize, entity: Entity) -> Result<usize, Status> {
    // SAFETY: `state` is the live store `store_of` found; its record array
    // pointer sits at the build table's `knowledge.records` offset.
    let records: usize = unsafe { read(state, layout.records) };
    // SAFETY: `state` is the live store `store_of` found; its u64 record
    // count sits at the build table's `knowledge.record_count` offset.
    let count: u64 = unsafe { read(state, layout.record_count) };
    let index = (entity.0 as usize).checked_sub(1).ok_or(Status::REFUSED)?;
    if records == 0 || index as u64 >= count {
        return Err(Status::REFUSED);
    }
    let record = records + index * layout.record_size;
    // SAFETY: `records` is non-null and `index` below the record count by
    // the check before it, so `record` is a record of the array, of the build
    // table's `knowledge.record_size`; its u8 active mark sits at
    // `knowledge.record_active`.
    let active: u8 = unsafe { read(record, layout.record_active) };
    if active == 0 {
        return Err(Status::REFUSED);
    }
    Ok(record)
}

/// Knowledge `id` of the character `entity`, or of the world.
pub(crate) fn value(loader: &Loader, entity: Entity, id: u32) -> Result<KnowledgeInfo, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (_, state) = store(layout)?;
    let (scope, def) = definition(layout, state, id).ok_or(Status::MISSING)?;
    let raw = if scope == KnowledgeInfo::WORLD || entity == Entity(0) {
        if scope == KnowledgeInfo::PLAYER {
            return Err(Status::REFUSED);
        }
        // SAFETY: `state` is the live store `store` found through
        // `world::server`, which put this call on the game thread at a quiet
        // moment; `id` lives on this stack for the call.
        unsafe { (layout.world_value)(core::ptr::with_exposed_provenance(state), &raw const id) }
    } else {
        record(layout, state, entity)?;
        // SAFETY: `state` is the live store `store` found through
        // `world::server`, which put this call on the game thread at a quiet
        // moment; `record` checked the character's record exists and is
        // active; `id` lives on this stack for the call.
        unsafe {
            (layout.player_value)(
                core::ptr::with_exposed_provenance(state),
                &raw const id,
                entity.0,
            )
        }
    };
    Ok(KnowledgeInfo {
        header: Header::of::<KnowledgeInfo>(),
        id,
        // SAFETY: `def` is the definition `definition` found in the live
        // blob; its kind sits at the reflected `type` field.
        kind: unsafe { read(def, layout.def_kind) },
        scope,
        reserved: 0,
        #[expect(clippy::cast_possible_truncation, reason = "the value is the low half")]
        value: raw as u32,
        // SAFETY: `def` is the definition `definition` found in the live
        // blob; its flags sit at the reflected `expectedFlags` field.
        flags: unsafe { read(def, layout.def_flags) },
    })
}

/// Queues a grant of `id` to the character `entity`, or to the world.
pub(crate) fn grant(
    loader: &Loader,
    entity: u32,
    id: u32,
    kind: u32,
    value: u32,
) -> Result<(), Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (game, state) = store(layout)?;
    let (scope, def) = definition(layout, state, id).ok_or(Status::BAD_ARGS)?;
    // SAFETY: `def` is the definition `definition` found in the live blob;
    // its u8 kind sits at the reflected `type` field.
    let def_kind: u8 = unsafe { read(def, layout.def_kind) };
    // SAFETY: `def` is the definition `definition` found in the live blob;
    // its u32 flags sit at the reflected `expectedFlags` field.
    let expected: u32 = unsafe { read(def, layout.def_flags) };
    if u32::from(def_kind) != kind {
        return Err(Status::BAD_ARGS);
    }
    if def_kind == KnowledgeInfo::FLAGS
        && (value >= 32 || (expected != 0 && expected & (1 << value) == 0))
    {
        return Err(Status::BAD_ARGS);
    }
    let target = if scope == KnowledgeInfo::PLAYER {
        record(layout, state, Entity(entity))?;
        if !changes::character_loaded(loader, Entity(entity)) {
            return Err(Status::REFUSED);
        }
        // One player: a 1 in the low byte and the character in the high
        // half, as the game targets a player.
        1 | ((entity & 0xffff) << 16)
    } else {
        0
    };
    let mut request = [0u8; REQUEST];
    let mut put = |at: usize, bytes: &[u8]| {
        if let Some(out) = request.get_mut(at..at + bytes.len()) {
            out.copy_from_slice(bytes);
        }
    };
    put(0, &id.to_le_bytes());
    put(layout.request_kind, &[def_kind]);
    put(layout.request_value, &value.to_le_bytes());
    put(layout.request_target, &target.to_le_bytes());
    // SAFETY: `game` is the live game object `store` found; its trigger
    // queue pointer sits at the build table's `knowledge.queue` offset.
    let queue: usize = unsafe { read(game, layout.queue) };
    if queue == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: `queue` is the game's non-null trigger queue by the check
    // before it, which the game's workers push into from any thread; the push
    // copies the 16 bytes of `request`, which live on this stack.
    let pushed = unsafe {
        (layout.push)(
            core::ptr::with_exposed_provenance_mut(queue),
            &raw const request,
        )
    };
    if pushed == 0 {
        return Err(Status::REFUSED);
    }
    Ok(())
}

/// The quest ids of the running game.
pub(crate) fn quests(loader: &Loader) -> Result<Vec<u32>, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (game, _) = store(layout)?;
    let mut ids: Vec<u32> = quest_set(layout, game)
        .map(|set| set.iter().copied().collect())
        .unwrap_or_default();
    ids.sort_unstable();
    Ok(ids)
}

/// The quest ids, read from the journal the first time it is there.
fn quest_set(layout: &Layout, game: usize) -> Option<&'static FxHashSet<u32>> {
    if let Some(set) = QUESTS.get() {
        return Some(set);
    }
    // SAFETY: `game` is the live game object `store_of` found; its journal
    // registry pointer sits at the build table's `knowledge.journal` offset.
    let journal: usize = unsafe { read(game, layout.journal) };
    if journal == 0 {
        return None;
    }
    // SAFETY: `journal` is the game's non-null journal registry by the check
    // before it; its blob pointer sits at the build table's
    // `knowledge.journal_blob` offset.
    let blob: usize = unsafe { read(journal, layout.journal_blob) };
    if blob == 0 {
        return None;
    }
    // SAFETY: `blob` is the journal's non-null blob by the check before it, and
    // its reflected `quests` field is a blob array header.
    let (start, count) = unsafe { blob_array(blob + layout.quests) };
    if start == 0 || count == 0 {
        return None;
    }
    let set = (0..count)
        // SAFETY: `i` is below the quest array's count, so this is a quest of
        // the array, of the reflected `JournalQuestResource` size; its u32
        // id sits at the reflected `entryId` field.
        .map(|i| unsafe { read::<u32>(start + i * layout.quest_size, layout.quest_id) })
        .collect();
    Some(QUESTS.get_or_init(|| set))
}

/// Raises the knowledge changes and completed quests of the last step, on
/// the game thread after it.
pub(crate) fn after_step(loader: &Loader) {
    if !crate::host::handles(Event::KNOWLEDGE_CHANGED)
        && !crate::host::handles(Event::QUEST_COMPLETED)
    {
        // A handler that arrives later hears the updates after its first look.
        if !UNSEEN.load(Ordering::Relaxed) {
            UNSEEN.store(true, Ordering::Relaxed);
        }
        return;
    }
    let Some(layout) = layout(loader) else {
        return;
    };
    let Ok((game, state)) = store(layout) else {
        return;
    };
    // SAFETY: `state` is the live store `store` found on the game thread;
    // its u32 update counter sits at the build table's `knowledge.step`
    // offset.
    let step: u32 = unsafe { read(state, layout.step) };
    let first_look = UNSEEN.load(Ordering::Relaxed);
    if first_look {
        UNSEEN.store(false, Ordering::Relaxed);
    }
    if SEEN_STEP.swap(step, Ordering::Relaxed) == step || first_look {
        return;
    }
    // SAFETY: `state` is the live store `store` found on the game thread;
    // its world log pointer sits at the build table's `knowledge.world_log`
    // offset.
    let log: usize = unsafe { read(state, layout.world_log) };
    // SAFETY: `state` is the live store `store` found on the game thread;
    // its u32 world log count sits at the build table's
    // `knowledge.world_log_count` offset.
    let count: u32 = unsafe { read(state, layout.world_log_count) };
    raise(loader, layout, game, Entity(0), log, count);
    // SAFETY: `state` is the live store `store` found on the game thread;
    // its record array pointer sits at the build table's `knowledge.records`
    // offset.
    let records: usize = unsafe { read(state, layout.records) };
    // SAFETY: `state` is the live store `store` found on the game thread;
    // its u64 record count sits at the build table's
    // `knowledge.record_count` offset.
    let record_count: u64 = unsafe { read(state, layout.record_count) };
    if records == 0 || record_count > MAX_RECORDS {
        return;
    }
    for index in 0..usize::try_from(record_count).unwrap_or(0) {
        let record = records + index * layout.record_size;
        // SAFETY: `records` is non-null by the check before it and `index` below
        // the record count, so `record` is a record of the array; its u8
        // active mark sits at the build table's `knowledge.record_active`
        // offset.
        let active: u8 = unsafe { read(record, layout.record_active) };
        if active == 0 {
            continue;
        }
        // SAFETY: `record` is an active record of the array; its log pointer
        // sits at the build table's `knowledge.record_log` offset.
        let log: usize = unsafe { read(record, layout.record_log) };
        // SAFETY: `record` is an active record of the array; its u32 log
        // count sits at the build table's `knowledge.record_log_count`
        // offset.
        let count: u32 = unsafe { read(record, layout.record_log_count) };
        let entity = u32::try_from(index + 1).unwrap_or(0);
        raise(loader, layout, game, Entity(entity), log, count);
    }
}

/// Raises the changes of one log, and the quests among a player's.
fn raise(loader: &Loader, layout: &Layout, game: usize, entity: Entity, log: usize, count: u32) {
    if log == 0 || count > MAX_LOG {
        return;
    }
    for i in 0..count as usize {
        let entry = log + i * layout.log_entry;
        // SAFETY: `log` is non-null and `i` below its count by the check
        // before it, so `entry` is an entry of the log, of the build table's
        // `knowledge.log_entry_size`, which starts with its u32 id.
        let id: u32 = unsafe { read(entry, 0) };
        // SAFETY: `entry` is an entry of the live log, its u32 new value
        // after the id.
        let value: u32 = unsafe { read(entry, 4) };
        if id == 0 {
            continue;
        }
        let data = KnowledgeData {
            header: Header::of::<KnowledgeData>(),
            entity: entity.0,
            id,
            value,
        };
        loader.emit(Event::KNOWLEDGE_CHANGED, None, (&raw const data).cast());
        if entity != Entity(0)
            && value != 0
            && quest_set(layout, game).is_some_and(|set| set.contains(&id))
        {
            let quest = QuestData {
                header: Header::of::<QuestData>(),
                entity: entity.0,
                quest: id,
            };
            loader.emit(Event::QUEST_COMPLETED, None, (&raw const quest).cast());
        }
    }
}

/// Forgets the last update seen when a hosted game ends.
pub(crate) fn forget() {
    SEEN_STEP.store(0, Ordering::Relaxed);
}
