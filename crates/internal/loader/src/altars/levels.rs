use enshwick::raw::{read_at, write_at};

use crate::ecs::{Engine, Query};

use super::geometry::Placed;
use super::{BUILT, LEVELS, PLACED};
use super::{
    FIELDS, HALF, MAX_LEVELS, Sizes, TARGETS, UPGRADE_ATTRIBUTES, UPGRADE_BALANCING,
    UPGRADE_ENTITY, UPGRADE_KIND, UPGRADE_LEVEL, UPGRADE_SHAPE, UPGRADE_STAMP_A, UPGRADE_STAMP_B,
    UPGRADE_TRACKER, UPGRADE_WORDS, ZONES_SHAPE, ZONES_TRANSFORM, ZONES_WORDS, drop_stale_buffers,
    merge,
};

/// Learns positions, orientations and built zones, after vanilla built them.
///
/// # Safety
///
/// - `ctx` is the live context of `add_buildzones_server`, used on its
///   thread while the system runs.
pub(super) unsafe fn observe(engine: &Engine, ctx: *mut u64) {
    drop_stale_buffers();
    PLACED.with_borrow_mut(|placed| {
        BUILT.with_borrow_mut(|built| {
            LEVELS.with_borrow_mut(|levels| {
                // SAFETY: the caller guarantees the live context of
                // `add_buildzones_server` on its thread.
                unsafe { observe_into(engine, ctx, placed, built) };
                merge(placed, built, levels);
            });
        });
    });
}

/// Walks `add_buildzones_server`'s rows again into `placed` and `built`.
///
/// # Safety
///
/// - `ctx` is the live context of `add_buildzones_server`, used on its
///   thread while the system runs.
pub(super) unsafe fn observe_into(
    engine: &Engine,
    ctx: *mut u64,
    placed: &mut Vec<(u32, Placed)>,
    built: &mut Vec<(u32, [f32; 3])>,
) {
    let Some(fields) = FIELDS.get() else {
        return;
    };
    // Vanilla left the row counter at the end: walk again from the start and
    // restore it.
    // SAFETY: the caller guarantees the system's live context on its thread.
    let saved = unsafe { engine.row(ctx) };
    // SAFETY: the caller guarantees the system's live context on its
    // thread, which alone writes it.
    unsafe { engine.set_row(ctx, 0) };
    // SAFETY: the caller guarantees the system's live context on its thread;
    // its iterator is `ZONES_WORDS` words, checked against the system's
    // reflected components at link; the query ends inside this call.
    let mut query: Query<'_, ZONES_WORDS> = unsafe { engine.query(ctx) };
    while let Some(words) = query.next() {
        let transform = words[ZONES_TRANSFORM] as *const u8;
        let shape = words[ZONES_SHAPE] as *const u8;
        if transform.is_null() || shape.is_null() {
            continue;
        }
        // SAFETY: the caller guarantees the system's live context on its
        // thread.
        let id = unsafe { engine.entity(ctx) };
        // SAFETY: `shape` is the row's non-null `TriggerShape` by the check
        // before it; its half extents sit at the reflected `rangeX` field.
        let range: [f32; 3] = unsafe { read_at(shape, fields.range) };
        // SAFETY: `transform` is the row's non-null `CurrentTransform` by
        // the check before it; its position sits at the reflected
        // `transform.position` field.
        let position: [i64; 3] = unsafe { read_at(transform, fields.position) };
        // SAFETY: `transform` is the row's non-null `CurrentTransform` by
        // the check before it; its orientation sits at the reflected
        // `transform.orientation` field.
        let orientation: [f32; 4] = unsafe { read_at(transform, fields.orientation) };
        built.push((id, range));
        placed.push((
            id,
            Placed {
                position,
                orientation,
            },
        ));
    }
    // SAFETY: the caller guarantees the system's live context on its
    // thread, which alone writes it.
    unsafe { engine.set_row(ctx, saved) };
}

/// Learns levels after vanilla's `upgrade_building_range`.
///
/// # Safety
///
/// - `ctx` is the live context of `upgrade_building_range`, used on its
///   thread while the system runs.
pub(super) unsafe fn read_levels(engine: &Engine, ctx: *mut u64) {
    drop_stale_buffers();
    LEVELS.with_borrow_mut(|levels| {
        // SAFETY: the caller guarantees the live context of
        // `upgrade_building_range` on its thread.
        unsafe { read_levels_into(engine, ctx, levels) };
        PLACED.with_borrow_mut(|placed| {
            BUILT.with_borrow_mut(|built| merge(placed, built, levels));
        });
    });
}

/// Walks `upgrade_building_range`'s rows again into `levels`.
///
/// # Safety
///
/// - `ctx` is the live context of `upgrade_building_range`, used on its
///   thread while the system runs.
pub(super) unsafe fn read_levels_into(
    engine: &Engine,
    ctx: *mut u64,
    levels: &mut Vec<(u32, Sizes)>,
) {
    // SAFETY: the caller guarantees the system's live context on its thread.
    let saved = unsafe { engine.row(ctx) };
    // SAFETY: the caller guarantees the system's live context on its
    // thread, which alone writes it.
    unsafe { engine.set_row(ctx, 0) };
    // SAFETY: the caller guarantees the system's live context on its thread;
    // its iterator is `UPGRADE_WORDS` words, checked against the system's
    // reflected components at link; the query ends inside this call.
    let mut query: Query<'_, UPGRADE_WORDS> = unsafe { engine.query(ctx) };
    while let Some(words) = query.next() {
        let words = *words;
        // SAFETY: `words` are the current row's iterator words of the live
        // query, on the system's thread.
        let Some(level) = (unsafe { altar_level(engine, &words) }) else {
            continue;
        };
        // SAFETY: `words` are the current row's iterator words of the live
        // query, on the system's thread.
        let Some((_, sizes)) = (unsafe { level_sizes(engine, &words, level) }) else {
            continue;
        };
        // SAFETY: the caller guarantees the system's live context on its
        // thread.
        levels.push((unsafe { engine.entity(ctx) }, sizes));
    }
    // SAFETY: the caller guarantees the system's live context on its
    // thread, which alone writes it.
    unsafe { engine.set_row(ctx, saved) };
}

/// Vanilla's `upgrade_building_range`, writing the controlling mod's ranges
/// where it has one for the altar's level.
///
/// # Safety
///
/// - `ctx` is the live context of `upgrade_building_range`, used on its
///   thread in place of the system's own run.
pub(super) unsafe fn write_ranges(engine: &Engine, ctx: *mut u64) {
    drop_stale_buffers();
    LEVELS.with_borrow_mut(|levels| {
        // SAFETY: the caller guarantees the live context of
        // `upgrade_building_range` on its thread, in place of its run.
        unsafe { write_ranges_into(engine, ctx, levels) };
        merge(&mut Vec::new(), &mut Vec::new(), levels);
    });
}

/// Writes each row's range as [`write_ranges`] describes, into `levels` the
/// sizes it saw.
///
/// # Safety
///
/// - `ctx` is the live context of `upgrade_building_range`, used on its
///   thread in place of the system's own run.
pub(super) unsafe fn write_ranges_into(
    engine: &Engine,
    ctx: *mut u64,
    levels: &mut Vec<(u32, Sizes)>,
) {
    let Some(range_at) = FIELDS.get().map(|fields| fields.range) else {
        return;
    };
    let targets = TARGETS.load();
    // SAFETY: the caller guarantees the system's live context on its thread;
    // its iterator is `UPGRADE_WORDS` words, checked against the system's
    // reflected components at link; the query ends inside this call.
    let mut query: Query<'_, UPGRADE_WORDS> = unsafe { engine.query(ctx) };
    while let Some(words) = query.next() {
        let words = *words;
        // SAFETY: `words` are the current row's iterator words of the live
        // query, on the system's thread.
        let Some(level) = (unsafe { altar_level(engine, &words) }) else {
            continue;
        };
        // SAFETY: `words` are the current row's iterator words of the live
        // query, on the system's thread.
        let Some((current, sizes)) = (unsafe { level_sizes(engine, &words, level) }) else {
            continue;
        };
        // SAFETY: the caller guarantees the system's live context on its
        // thread.
        let id = unsafe { engine.entity(ctx) };
        let target = match targets.as_deref().and_then(|targets| targets.get(&id)) {
            Some(&(solved_level, range)) if solved_level == level => range,
            _ => current.map(|v| v * HALF),
        };
        levels.push((id, sizes));
        let shape = words[UPGRADE_SHAPE] as *mut u8;
        if shape.is_null() {
            continue;
        }
        // SAFETY: `shape` is the row's non-null `TriggerShape` by the check
        // before it; its half extents sit at the reflected `rangeX` field.
        let written: [f32; 3] = unsafe { read_at(shape, range_at) };
        if written.map(f32::to_bits) != target.map(f32::to_bits) {
            // SAFETY: the tracker, stamps, entity and kind are the current
            // row's words of the live query, whose system writes
            // `TriggerShape`, on its thread.
            unsafe {
                engine.mark_changed(
                    words[UPGRADE_TRACKER],
                    words[UPGRADE_STAMP_A],
                    words[UPGRADE_STAMP_B],
                    words[UPGRADE_ENTITY],
                    words[UPGRADE_KIND],
                );
            }
            // SAFETY: `shape` is the row's non-null `TriggerShape`, which
            // this system writes, on its thread in place of its run; the half
            // extents sit at the reflected `rangeX` field.
            unsafe { write_at(shape, range_at, target) };
        }
    }
}

/// The altar level of the current row, or `None` for a row that is no altar.
///
/// # Safety
///
/// - `words` are the current row's iterator words of a live
///   `upgrade_building_range` query, used on its system's thread.
pub(super) unsafe fn altar_level(engine: &Engine, words: &[u64; UPGRADE_WORDS]) -> Option<usize> {
    let level_component = words[UPGRADE_LEVEL] as *const u8;
    // SAFETY: the caller guarantees the current row's words, so these are
    // the row's null or live `Level` attribute set and its attribute
    // registry, on the system's thread.
    let value = unsafe { engine.level(words[UPGRADE_ATTRIBUTES], level_component) }?;
    // The level is the attribute's low byte.
    Some(usize::from(value.to_le_bytes()[0]))
}

/// The current level's size and the sizes of every level up to it; `None`
/// when a size is missing.
///
/// # Safety
///
/// - `words` are the current row's iterator words of a live
///   `upgrade_building_range` query, used on its system's thread.
pub(super) unsafe fn level_sizes(
    engine: &Engine,
    words: &[u64; UPGRADE_WORDS],
    level: usize,
) -> Option<([f32; 3], Sizes)> {
    if level > MAX_LEVELS {
        return None;
    }
    // SAFETY: the caller guarantees the current row's words, so this is the
    // live row's balancing registry word, on the system's thread.
    let size_of = |l: usize| unsafe { engine.zone_size(words[UPGRADE_BALANCING], l) };
    let mut sizes = Sizes::new();
    for l in 1..=level {
        sizes.try_push(size_of(l)?).ok()?;
    }
    let current = match sizes.last() {
        Some(&current) => current,
        None => size_of(level)?,
    };
    Some((current, sizes))
}
