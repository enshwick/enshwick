//! Survivors per altar on the client.
//!
//! - The cursor check measures from the first copy of a survivor the client
//!   knows, often one at another altar, and refuses a bed with "too far"; it is
//!   told that no copy exists, as the server is.
//! - The client keeps one UI record per survivor and every copy writes its
//!   status bits into it in turn. After vanilla's pass the record keeps the
//!   bits every copy has, the worst status, so a copy without a bed shows "no
//!   bed". Only that one UI byte is written.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use enshwick_api::{Load, Next, Off};

type IteratorInit = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64);
type IteratorNext = unsafe extern "system" fn(ctx: *mut u64, iterator: *mut u64, size: u64) -> u64;
type Template = unsafe extern "system" fn(ctx: *mut u64) -> *const [u8; 16];
type LookupFn = unsafe extern "system" fn(*mut u8, *const u32) -> u32;
type SystemFn = unsafe extern "system" fn(*mut u64);

/// `ui_npc_status`'s iterator: a header, `ClientActiveNpcState`,
/// `NetworkComfort`, `RenderTransform`, UI offsets, `UiPlayData`.
const ITERATOR_WORDS: usize = 6;
const ITERATOR_STATE: usize = 1;
const ITERATOR_PLAY_DATA: usize = 5;

#[derive(Debug)]
struct Engine {
    init: IteratorInit,
    next: IteratorNext,
    template: Template,
    cursor_return: u64,
    state_status: usize,
    map_bits: usize,
    map_capacity: usize,
    map_keys: usize,
    map_values: usize,
    map_count: usize,
    record_size: usize,
    record_status: usize,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();

enshwick_api::detour! {
    static CURSOR_LOOKUP: unsafe extern "system" fn(bases: *mut u8, npc: *const u32) -> u32 = cursor_lookup,
        flags = enshwick_api::abi::HookFlags::CALLER_FRAME;
}
enshwick_api::detour! {
    static UI_STATUS: unsafe extern "system" fn(ctx: *mut u64) = ui_status;
}

/// Turns a resolved address into a function pointer of type `F`.
///
/// # Safety
///
/// `address` is a function of type `F`.
unsafe fn function<F: Copy>(address: u64) -> F {
    let address = usize::try_from(address).unwrap_or(0);
    // SAFETY: forwarded from the caller; `F` is a function pointer.
    unsafe { core::mem::transmute_copy(&address) }
}

/// Links the client hooks.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    let size = |name| {
        load.constant(name)
            .map(|v| usize::try_from(v).unwrap_or(usize::MAX))
    };
    let (init, next, template) = (
        load.symbol("ecs.iterator_init")?,
        load.symbol("ecs.iterator_next")?,
        load.symbol("ecs.template_guid")?,
    );
    let engine = Engine {
        // SAFETY: the loader pins these ids to functions of these types.
        init: unsafe { function(init) },
        // SAFETY: as above.
        next: unsafe { function(next) },
        // SAFETY: as above.
        template: unsafe { function(template) },
        cursor_return: load.symbol("survivors.cursor_return")?,
        state_status: size("survivors.state_status")?,
        map_bits: size("survivors.ui_map_bits")?,
        map_capacity: size("survivors.ui_map_capacity")?,
        map_keys: size("survivors.ui_map_keys")?,
        map_values: size("survivors.ui_map_values")?,
        map_count: size("survivors.ui_map_count")?,
        record_size: size("survivors.ui_record_size")?,
        record_status: size("survivors.ui_record_status")?,
    };
    let _first = ENGINE.set(engine);
    load.hook(&CURSOR_LOOKUP, load.symbol("npc.lookup_cursor")?)?;
    load.hook(&UI_STATUS, load.system("ui_npc_status")?)
}

fn cursor_lookup(next: Next<LookupFn>, bases: *mut u8, npc: *const u32) -> u32 {
    if let (Some(engine), Some(host)) = (ENGINE.get(), enshwick_api::host())
        // SAFETY: the frame was taken at entry of this chain.
        && unsafe { host.caller_frame().return_address() } == engine.cursor_return
    {
        return 0;
    }
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { next.get()(bases, npc) }
}

fn ui_status(next: Next<SystemFn>, ctx: *mut u64) {
    // SAFETY: the original system takes its own context.
    unsafe { next.get()(ctx) };
    if let Some(engine) = ENGINE.get()
        && !ctx.is_null()
    {
        // SAFETY: the system's live context, after vanilla's pass.
        unsafe { worst_status(engine, ctx) };
    }
}

/// Reads a `T` at `offset` bytes past `base`.
///
/// # Safety
///
/// A readable `T` lies there.
unsafe fn read_at<T: Copy>(base: usize, offset: usize) -> T {
    // SAFETY: forwarded from the caller.
    unsafe { (base.wrapping_add(offset) as *const T).read_unaligned() }
}

/// # Safety
///
/// `ctx` is `ui_npc_status`'s live context.
unsafe fn worst_status(engine: &Engine, ctx: *mut u64) {
    // Survivor id to the bits every copy has and how many copies were seen.
    let mut status: BTreeMap<u32, (u8, u32)> = BTreeMap::new();
    let mut play_data = 0usize;
    let size = (ITERATOR_WORDS * 8) as u64;
    let mut words = [0u64; ITERATOR_WORDS];
    // SAFETY: the system's live context and its iterator size.
    unsafe { (engine.init)(ctx, words.as_mut_ptr(), size) };
    // SAFETY: as above.
    while unsafe { (engine.next)(ctx, words.as_mut_ptr(), size) } & 0xff != 0 {
        let state = usize::try_from(words[ITERATOR_STATE]).unwrap_or(0);
        // SAFETY: the current row's template.
        let guid = unsafe { (engine.template)(ctx) };
        if state == 0 || guid.is_null() {
            continue;
        }
        play_data = usize::try_from(words[ITERATOR_PLAY_DATA]).unwrap_or(0);
        // SAFETY: the row's `ClientActiveNpcState` status bits.
        let bits: u8 = unsafe { read_at(state, engine.state_status) };
        // SAFETY: a template GUID is 16 bytes.
        let id = survivor_id(unsafe { &*guid });
        let entry = status.entry(id).or_insert((u8::MAX, 0));
        entry.0 &= bits;
        entry.1 += 1;
    }
    if play_data == 0 {
        return;
    }
    for (id, (bits, copies)) in status {
        // SAFETY: the play data's survivor record map.
        if copies >= 2
            // SAFETY: the play data's survivor record map.
            && let Some(record) = unsafe { record(engine, play_data, id) }
        {
            // SAFETY: the record's status byte, which the UI reads.
            unsafe { (record.wrapping_add(engine.record_status) as *mut u8).write(bits) };
        }
    }
}

/// The UI record of survivor `id`, found as vanilla finds it.
///
/// # Safety
///
/// `play_data` is the client's `UiPlayData`.
unsafe fn record(engine: &Engine, play_data: usize, id: u32) -> Option<usize> {
    // SAFETY: the map's capacity.
    let capacity: u64 = unsafe { read_at(play_data, engine.map_capacity) };
    // SAFETY: its entry count.
    let count: u32 = unsafe { read_at(play_data, engine.map_count) };
    // SAFETY: its occupancy bitset.
    let bits: usize = unsafe { read_at(play_data, engine.map_bits) };
    // SAFETY: its keys.
    let keys: usize = unsafe { read_at(play_data, engine.map_keys) };
    // SAFETY: its values.
    let values: usize = unsafe { read_at(play_data, engine.map_values) };
    let capacity = usize::try_from(capacity).ok()?;
    if count == 0 || capacity == 0 || bits == 0 || keys == 0 || values == 0 {
        return None;
    }
    // SAFETY: the occupancy bitset has a bit per slot.
    let used = |i: usize| unsafe { read_at::<u64>(bits, (i >> 6) * 8) } >> (i & 63) & 1 != 0;
    let mut i = (capacity - 1) & id as usize;
    for _ in 0..capacity {
        if !used(i) {
            return None;
        }
        // SAFETY: an occupied slot's key.
        if unsafe { read_at::<u32>(keys, i * 4) } == id {
            return Some(values + i * engine.record_size);
        }
        i = if i + 1 < capacity { i + 1 } else { 0 };
    }
    None
}

/// A survivor's id: FNV-1a over its template's GUID, 0 for an empty GUID.
fn survivor_id(guid: &[u8; 16]) -> u32 {
    if guid.iter().all(|&b| b == 0) {
        return 0;
    }
    enshwick_api::content::Guid(*guid).id()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn survivor_ids_are_fnv1a_over_the_guid() {
        assert_eq!(survivor_id(&[0; 16]), 0);
        let mut expected = 0x811c_9dc5_u32;
        for _ in 0..16 {
            expected = (expected ^ 1).wrapping_mul(0x0100_0193);
        }
        assert_eq!(survivor_id(&[1; 16]), expected);
    }
}
