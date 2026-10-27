use enshwick::raw::{read_at, read_ptr_at, write_at};

use super::{
    ItemMap, LOADED, MENU_NEXT, Menu, REGISTRY_NEXT, SEALED, STATUS_ROWS, SURVIVORS_FUSE,
    UI_PLAY_DATA, UI_STATE, UI_WORDS, item_map, menu,
};

pub(super) unsafe extern "system" fn registry_link(registry: *mut u8) {
    let ready = LOADED
        .get()
        .and_then(|loader| item_map(loader))
        .zip(SEALED.get())
        .filter(|_| !registry.is_null());
    // The game calls this often and builds only once: a registry that holds
    // its collection already is left as it is.
    // SAFETY: `ready` holds only for the non-null registry the game passes
    // its load, live for the call, which keeps its collection pointer, null
    // until it is built, at the table's `survivors.registry_loaded`.
    let built =
        ready.is_some_and(|(m, _)| !unsafe { read_ptr_at::<u8>(registry, m.loaded) }.is_null());
    // SAFETY: the game's own argument, passed on unchanged to the rest of the
    // chain, which has the registry load's signature.
    unsafe { REGISTRY_NEXT.get()(registry) };
    let Some((map, sealed)) = ready.filter(|_| !built) else {
        return;
    };
    let _filed = SURVIVORS_FUSE.run("the staff item filing", || {
        let mut filed = 0;
        for &(item, survivor) in &sealed.items {
            // SAFETY: the game's load just built the non-null registry, whose
            // staff item map lies at the table's `survivors.registry_item_map`;
            // the load's thread alone uses it during the call.
            if unsafe { file(map, registry.wrapping_add(map.at), item, survivor) } {
                filed += 1;
            }
        }
        crate::log!(
            INFO,
            "survivors: filed {filed} staff items in the survivor registry"
        );
    });
}

/// Maps `item` to `survivor` in the game's map at `table`, as its own build
/// does: the game's find-or-free-slot, which grows the table as needed, then
/// the bit, key and value.
///
/// # Safety
///
/// - `table` is the live staff item map, laid out at `map`'s offsets: a
///   `u64` capacity, an occupancy bitset of a bit per slot, and `u32` keys
///   and values of `capacity` slots each.
/// - `map.find_slot` is the game's find-or-free-slot for that map.
/// - No other thread uses the map during the call.
pub(super) unsafe fn file(map: &ItemMap, table: *mut u8, item: u32, survivor: u32) -> bool {
    // SAFETY: the caller's contract makes `map.find_slot` the game's own slot
    // search for the live `table`, used by no other thread.
    let slot = unsafe { (map.find_slot)(table, &raw const item) };
    // SAFETY: the caller's contract makes `table` the live map, holding its
    // `u64` capacity at `map.capacity`; read after the search, which may grow
    // the map.
    let capacity: u64 = unsafe { read_at(table, map.capacity) };
    if slot >= capacity {
        return false;
    }
    let Ok(slot) = usize::try_from(slot) else {
        return false;
    };
    // SAFETY: the caller's contract makes `table` the live map, holding its
    // bitset pointer at `map.bits`; read after a possible growth.
    let bits = unsafe { read_ptr_at::<u8>(table, map.bits) };
    // SAFETY: the caller's contract makes `table` the live map, holding its
    // keys pointer at `map.keys`; read after a possible growth.
    let keys = unsafe { read_ptr_at::<u8>(table, map.keys) };
    // SAFETY: the caller's contract makes `table` the live map, holding its
    // values pointer at `map.values`; read after a possible growth.
    let values = unsafe { read_ptr_at::<u8>(table, map.values) };
    let word = (slot >> 6) * size_of::<u64>();
    let bit = 1u64 << (slot & 63);
    // SAFETY: `slot` is below the capacity (checked earlier), so its `u64`
    // word lies in the map's bitset of a bit per slot.
    let marks: u64 = unsafe { read_at(bits, word) };
    if marks & bit == 0 {
        // SAFETY: `slot` is below the capacity (checked earlier), so its `u64`
        // word lies in the map's bitset, which no other thread uses.
        unsafe { write_at(bits, word, marks | bit) };
        // SAFETY: the caller's contract makes `table` the live map, holding
        // its `u32` entry count at `map.count`.
        let count: u32 = unsafe { read_at(table, map.count) };
        // SAFETY: the caller's contract makes `table` the live map, holding
        // its `u32` entry count at `map.count`, which no other thread uses.
        unsafe { write_at(table, map.count, count + 1) };
        // SAFETY: `slot` is below the capacity (checked earlier), so its `u32`
        // key lies in the map's keys, which no other thread uses.
        unsafe { write_at(keys, slot * size_of::<u32>(), item) };
    }
    // SAFETY: `slot` is below the capacity (checked earlier), so its `u32`
    // value lies in the map's values, which no other thread uses.
    unsafe { write_at(values, slot * size_of::<u32>(), survivor) };
    true
}

pub(super) unsafe extern "system" fn menu_link(ctx: *mut u64) {
    // SAFETY: the game's own argument, passed on unchanged to the rest of the
    // chain, which has the system's signature.
    unsafe { MENU_NEXT.get()(ctx) };
    if ctx.is_null() {
        return;
    }
    // SAFETY: the game passes `ui_npc_status` its non-null (checked earlier)
    // live context, used here on the system's thread after vanilla's pass,
    // while the system still runs.
    let _done = SURVIVORS_FUSE.run("the status merge", || unsafe { merge_statuses(ctx) });
}

/// Hands every survivor's statuses to the merging mod and writes its answer
/// into the menu record.
///
/// # Safety
///
/// - `ctx` is `ui_npc_status`'s live context, whose query iterator is
///   [`UI_WORDS`] words over [`UI_READS`].
/// - The call runs on the system's thread while the system runs.
pub(super) unsafe fn merge_statuses(ctx: *mut u64) {
    let (Some(loader), Some(engine), Some(sealed)) =
        (LOADED.get(), crate::ecs::resolved(), SEALED.get())
    else {
        return;
    };
    let (Some(menu), Some(merge)) = (menu(loader), sealed.status) else {
        return;
    };
    if crate::host::is_off(merge.owner) {
        return;
    }
    STATUS_ROWS.with_borrow_mut(|(rows, bits)| {
        rows.clear();
        let mut play_data = 0usize;
        // Vanilla left the row counter at the end: walk again from the start
        // and restore it, as the altars' walks do.
        // SAFETY: the caller's contract makes `ctx` the system's live context
        // on its thread.
        let saved = unsafe { engine.row(ctx) };
        // SAFETY: the caller's contract makes `ctx` the system's live context
        // on its thread, which alone writes it while the system runs.
        unsafe { engine.set_row(ctx, 0) };
        // SAFETY: the caller's contract makes `ctx` the system's live context
        // on its thread with an iterator of `UI_WORDS` words; the query is
        // dropped at the end of this closure, inside the system's run.
        let mut query = unsafe { engine.query::<UI_WORDS>(ctx) };
        while let Some(words) = query.next() {
            let Ok(state) = usize::try_from(words[UI_STATE]) else {
                continue;
            };
            // SAFETY: the caller's contract makes `ctx` the system's live
            // context, on the row the query just moved to.
            let guid = unsafe { (menu.template)(ctx) };
            if state == 0 || guid.is_null() {
                continue;
            }
            play_data = usize::try_from(words[UI_PLAY_DATA]).unwrap_or(0);
            // SAFETY: `state` is the row's non-zero live
            // `ClientActiveNpcState` (checked earlier), which keeps its status
            // bits at the table's `survivors.state_status`.
            let status: u8 = unsafe { crate::memory::read(state, menu.state_status) };
            // SAFETY: the template lookup returned a non-null (checked earlier)
            // pointer to the row's 16-byte template GUID.
            let guid: [u8; 16] = unsafe { read_at(guid.cast(), 0) };
            if guid.iter().all(|&b| b == 0) {
                continue;
            }
            let id = enshwick::content::Guid(guid).fnv1a();
            let order = u32::try_from(rows.len()).unwrap_or(u32::MAX);
            rows.push((id, order, status));
        }
        // SAFETY: the system's live context.
        unsafe { engine.set_row(ctx, saved) };
        if play_data == 0 {
            return;
        }
        rows.sort_unstable();
        let user = core::ptr::with_exposed_provenance_mut(merge.user);
        for group in rows.chunk_by(|a, b| a.0 == b.0) {
            let id = group[0].0;
            bits.clear();
            bits.extend(group.iter().map(|&(_, _, status)| status));
            // SAFETY: the registered `SurvivorStatusFn` lives in the mod's
            // DLL, which stays loaded for the process, and takes the `user`
            // word it registered; `bits` outlives the call.
            let answer =
                crate::profile::call(merge.owner, crate::profile::Feature::Survivors, || unsafe {
                    (merge.callback)(id, bits.as_ptr(), bits.len(), user)
                });
            if answer & enshwick::abi::SURVIVOR_STATUS_SET == 0 {
                continue;
            }
            // SAFETY: `play_data` is the client's non-zero (checked earlier)
            // live `UiPlayData` from the system's own query words, used on
            // its thread while it runs.
            if let Some(record) = unsafe { record(menu, play_data, id) } {
                // SAFETY: `record` is a live menu record of the play data's
                // map, which keeps the status byte the menu reads at the
                // table's `survivors.ui_record_status`, on the system's
                // thread.
                unsafe { write_at(record, menu.record_status, answer.to_le_bytes()[0]) };
            }
        }
    });
}

/// The menu record of survivor `id`, found as vanilla finds it.
///
/// # Safety
///
/// - `play_data` is the address of the client's live `UiPlayData`, whose
///   survivor record map lies at `menu`'s offsets: a power-of-two
///   capacity, an occupancy bitset of a bit per slot, and `u32` keys and
///   records of `capacity` slots each.
/// - Nothing writes the map during the call.
pub(super) unsafe fn record(menu: &Menu, play_data: usize, id: u32) -> Option<*mut u8> {
    // SAFETY: the caller's contract makes `play_data` the live play data,
    // which keeps the map's `u64` capacity at the table's
    // `survivors.ui_map_capacity`.
    let capacity: u64 = unsafe { crate::memory::read(play_data, menu.map_capacity) };
    // SAFETY: the caller's contract makes `play_data` the live play data,
    // which keeps the map's `u32` entry count at the table's
    // `survivors.ui_map_count`.
    let count: u32 = unsafe { crate::memory::read(play_data, menu.map_count) };
    // SAFETY: the caller's contract makes `play_data` the live play data,
    // which keeps the map's bitset pointer at the table's
    // `survivors.ui_map_bits`.
    let bits = unsafe { crate::memory::read_ptr::<u8>(play_data, menu.map_bits) }.cast_const();
    // SAFETY: the caller's contract makes `play_data` the live play data,
    // which keeps the map's keys pointer at the table's
    // `survivors.ui_map_keys`.
    let keys = unsafe { crate::memory::read_ptr::<u8>(play_data, menu.map_keys) }.cast_const();
    // SAFETY: the caller's contract makes `play_data` the live play data,
    // which keeps the map's records pointer at the table's
    // `survivors.ui_map_values`.
    let values = unsafe { crate::memory::read_ptr::<u8>(play_data, menu.map_values) };
    if count == 0 || capacity == 0 || bits.is_null() || keys.is_null() || values.is_null() {
        return None;
    }
    let capacity = usize::try_from(capacity).ok()?;
    let used = |i: usize| {
        // SAFETY: `i` is below the non-zero capacity, and the caller's
        // contract gives the non-null bitset (checked earlier) a bit per slot.
        let marks: u64 = unsafe { read_at(bits, (i >> 6) * size_of::<u64>()) };
        marks >> (i & 63) & 1 != 0
    };
    let mut i = (capacity - 1) & usize::try_from(id).ok()?;
    for _ in 0..capacity {
        if !used(i) {
            return None;
        }
        // SAFETY: `i` is below the capacity, and the caller's contract gives
        // the non-null keys (checked earlier) a `u32` per slot.
        if unsafe { read_at::<u32>(keys, i * size_of::<u32>()) } == id {
            return Some(values.wrapping_add(i * menu.record_size));
        }
        i = if i + 1 < capacity { i + 1 } else { 0 };
    }
    None
}
