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
use std::sync::atomic::{AtomicU32, Ordering};

use enshwick_loader_api::abi::{Event, Header, KnowledgeData, KnowledgeInfo, QuestData, Status};
use rustc_hash::FxHashSet;

use crate::host::Loader;

pub(crate) mod guard;

type PlayerValueFn =
    unsafe extern "system" fn(state: *const u8, id: *const u32, entity: u32) -> u64;
type WorldValueFn = unsafe extern "system" fn(state: *const u8, id: *const u32) -> u64;
type PushFn = unsafe extern "system" fn(queue: *mut u8, request: *const [u8; 16]) -> u8;

/// More player records than a server keeps: a count past it is a misread.
const MAX_RECORDS: u64 = 4096;
/// More changes than one update logs: a count past it is a misread.
const MAX_LOG: u32 = 1 << 16;

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
            let function = |name: &str| {
                loader
                    .address(name)
                    .and_then(|address| usize::try_from(address).ok())
            };
            let offset = |name: &str| loader.offset(name);
            let (player_value, world_value, push) = (
                function("knowledge.player_value")?,
                function("knowledge.world_value")?,
                function("knowledge.push")?,
            );
            let defs = "keen::GameKnowledgeResource";
            let def = "keen::KnowledgeObjectResource";
            Some(Layout {
                // SAFETY: each symbol is the game function with this
                // signature on this build.
                player_value: unsafe { core::mem::transmute::<usize, PlayerValueFn>(player_value) },
                // SAFETY: as above.
                world_value: unsafe { core::mem::transmute::<usize, WorldValueFn>(world_value) },
                // SAFETY: as above.
                push: unsafe { core::mem::transmute::<usize, PushFn>(push) },
                game: offset("server.player_infos")?,
                object: offset("knowledge.object")?,
                state: offset("knowledge.state")?,
                defs: offset("knowledge.defs")?,
                defs_blob: offset("knowledge.defs_blob")?,
                queue: offset("knowledge.queue")?,
                records: offset("knowledge.records")?,
                record_count: offset("knowledge.record_count")?,
                record_size: offset("knowledge.record_size")?,
                record_active: offset("knowledge.record_active")?,
                step: offset("knowledge.step")?,
                world_log: offset("knowledge.world_log")?,
                world_log_count: offset("knowledge.world_log_count")?,
                record_log: offset("knowledge.record_log")?,
                record_log_count: offset("knowledge.record_log_count")?,
                log_entry: offset("knowledge.log_entry_size")?,
                journal: offset("knowledge.journal")?,
                journal_blob: offset("knowledge.journal_blob")?,
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

/// Reads a `T` at `base + at`.
///
/// # Safety
///
/// The address holds a readable `T`.
unsafe fn read<T: Copy>(base: usize, at: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { crate::world::read(base, at) }
}

/// A blob array at `header`: where its elements start and how many.
///
/// # Safety
///
/// `header` is a live blob array header: an offset from itself and a count.
unsafe fn blob_array(header: usize) -> (usize, usize) {
    // SAFETY: forwarded from the caller.
    let offset: i32 = unsafe { read(header, 0) };
    // SAFETY: as above.
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

/// The game object and its knowledge store of `server`.
fn store_of(layout: &Layout, server: usize) -> Result<(usize, usize), Status> {
    // SAFETY: the server holds its game object.
    let game: usize = unsafe { read(server, layout.game) };
    if game == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the game object holds its knowledge object from its start.
    let object: usize = unsafe { read(game, layout.object) };
    if object == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the knowledge object holds its store.
    let state: usize = unsafe { read(object, layout.state) };
    if state == 0 {
        return Err(Status::MISSING);
    }
    Ok((game, state))
}

/// The definition of `id`: its scope and its data, or `None` for an id the
/// game does not know.
fn definition(layout: &Layout, state: usize, id: u32) -> Option<(u8, usize)> {
    // SAFETY: the store holds its definitions' holder, which holds the blob.
    let holder: usize = unsafe { read(state, layout.defs) };
    if holder == 0 {
        return None;
    }
    // SAFETY: as above.
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
    // SAFETY: a live map of the definitions' blob.
    let (buckets, bucket_count) = unsafe { blob_array(map) };
    if bucket_count == 0 || buckets == 0 {
        return None;
    }
    let bucket = id as usize & (bucket_count - 1);
    // SAFETY: the bucket lies inside the bucket array.
    let start: u32 = unsafe { read(buckets, bucket * 8) };
    // SAFETY: as above.
    let count: u32 = unsafe { read(buckets, bucket * 8 + 4) };
    // SAFETY: the map's key and value arrays follow its bucket array.
    let (keys, _) = unsafe { blob_array(map + 8) };
    // SAFETY: as above.
    let (values, _) = unsafe { blob_array(map + 16) };
    (start..start.saturating_add(count))
        .map(|i| i as usize)
        // SAFETY: a key of the bucket.
        .find(|&i| unsafe { read::<u32>(keys, i * 4) } == id)
        .map(|i| values + i * stride)
}

/// The player record of the character `entity`, active, or why not.
fn record(layout: &Layout, state: usize, entity: u32) -> Result<usize, Status> {
    // SAFETY: the store holds its player records and their number.
    let records: usize = unsafe { read(state, layout.records) };
    // SAFETY: as above.
    let count: u64 = unsafe { read(state, layout.record_count) };
    let index = (entity as usize).checked_sub(1).ok_or(Status::REFUSED)?;
    if records == 0 || index as u64 >= count {
        return Err(Status::REFUSED);
    }
    let record = records + index * layout.record_size;
    // SAFETY: a record of the array.
    let active: u8 = unsafe { read(record, layout.record_active) };
    if active == 0 {
        return Err(Status::REFUSED);
    }
    Ok(record)
}

/// Knowledge `id` of the character `entity`, or of the world.
pub(crate) fn value(loader: &Loader, entity: u32, id: u32) -> Result<KnowledgeInfo, Status> {
    let layout = layout(loader).ok_or(Status::MISSING)?;
    let (_, state) = store(layout)?;
    let (scope, def) = definition(layout, state, id).ok_or(Status::MISSING)?;
    let raw = if scope == KnowledgeInfo::WORLD || entity == 0 {
        if scope == KnowledgeInfo::PLAYER {
            return Err(Status::REFUSED);
        }
        // SAFETY: the game's getter on its live store, on the game thread.
        unsafe { (layout.world_value)(core::ptr::with_exposed_provenance(state), &raw const id) }
    } else {
        record(layout, state, entity)?;
        // SAFETY: as above, for a player record that exists and is active.
        unsafe {
            (layout.player_value)(
                core::ptr::with_exposed_provenance(state),
                &raw const id,
                entity,
            )
        }
    };
    Ok(KnowledgeInfo {
        header: Header::of::<KnowledgeInfo>(),
        id,
        // SAFETY: the definition's fields, as reflected.
        kind: unsafe { read(def, layout.def_kind) },
        scope,
        reserved: 0,
        #[expect(clippy::cast_possible_truncation, reason = "the value is the low half")]
        value: raw as u32,
        // SAFETY: as above.
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
    // SAFETY: the definition's fields, as reflected.
    let def_kind: u8 = unsafe { read(def, layout.def_kind) };
    // SAFETY: as above.
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
        record(layout, state, entity)?;
        // One player: a 1 in the low byte and the character in the high
        // half, as the game targets a player.
        1 | ((entity & 0xffff) << 16)
    } else {
        0
    };
    let mut request = [0u8; 16];
    request[0..4].copy_from_slice(&id.to_le_bytes());
    request[4] = def_kind;
    request[8..12].copy_from_slice(&value.to_le_bytes());
    request[12..16].copy_from_slice(&target.to_le_bytes());
    // SAFETY: the game object holds its knowledge trigger queue.
    let queue: usize = unsafe { read(game, layout.queue) };
    if queue == 0 {
        return Err(Status::MISSING);
    }
    // SAFETY: the game's push into its own queue, safe from any thread.
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
    // SAFETY: the game object holds its journal registry, which holds the
    // journal's blob.
    let journal: usize = unsafe { read(game, layout.journal) };
    if journal == 0 {
        return None;
    }
    // SAFETY: as above.
    let blob: usize = unsafe { read(journal, layout.journal_blob) };
    if blob == 0 {
        return None;
    }
    // SAFETY: the blob's quest array.
    let (start, count) = unsafe { blob_array(blob + layout.quests) };
    if start == 0 || count == 0 {
        return None;
    }
    let set = (0..count)
        // SAFETY: a quest of the array; its id as reflected.
        .map(|i| unsafe { read::<u32>(start + i * layout.quest_size, layout.quest_id) })
        .collect();
    Some(QUESTS.get_or_init(|| set))
}

/// Raises the knowledge changes and completed quests of the last step, on
/// the game thread after it.
pub(crate) fn after_step(loader: &Loader) {
    let Some(layout) = layout(loader) else {
        return;
    };
    let Ok((game, state)) = store(layout) else {
        return;
    };
    // SAFETY: the store's update counter.
    let step: u32 = unsafe { read(state, layout.step) };
    if SEEN_STEP.swap(step, Ordering::Relaxed) == step {
        return;
    }
    // SAFETY: the store's world log of the last update.
    let log: usize = unsafe { read(state, layout.world_log) };
    // SAFETY: as above.
    let count: u32 = unsafe { read(state, layout.world_log_count) };
    raise(loader, layout, game, 0, log, count);
    // SAFETY: the store's player records and their number.
    let records: usize = unsafe { read(state, layout.records) };
    // SAFETY: as above.
    let record_count: u64 = unsafe { read(state, layout.record_count) };
    if records == 0 || record_count > MAX_RECORDS {
        return;
    }
    for index in 0..usize::try_from(record_count).unwrap_or(0) {
        let record = records + index * layout.record_size;
        // SAFETY: a record of the array.
        let active: u8 = unsafe { read(record, layout.record_active) };
        if active == 0 {
            continue;
        }
        // SAFETY: the record's log of the last update.
        let log: usize = unsafe { read(record, layout.record_log) };
        // SAFETY: as above.
        let count: u32 = unsafe { read(record, layout.record_log_count) };
        let entity = u32::try_from(index + 1).unwrap_or(0);
        raise(loader, layout, game, entity, log, count);
    }
}

/// Raises the changes of one log, and the quests among a player's.
fn raise(loader: &Loader, layout: &Layout, game: usize, entity: u32, log: usize, count: u32) {
    if log == 0 || count > MAX_LOG {
        return;
    }
    for i in 0..count as usize {
        let entry = log + i * layout.log_entry;
        // SAFETY: an entry of the log: its id and the new value.
        let id: u32 = unsafe { read(entry, 0) };
        // SAFETY: as above.
        let value: u32 = unsafe { read(entry, 4) };
        if id == 0 {
            continue;
        }
        let data = KnowledgeData {
            header: Header::of::<KnowledgeData>(),
            entity,
            id,
            value,
        };
        loader.emit(Event::KNOWLEDGE_CHANGED, None, (&raw const data).cast());
        if entity != 0 && value != 0 && quest_set(layout, game).is_some_and(|set| set.contains(&id))
        {
            let quest = QuestData {
                header: Header::of::<QuestData>(),
                entity,
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
