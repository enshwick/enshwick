//! Extra maximum health, stamina and mana, given the way gear gives them:
//! a change of the set's `*_Max_Adder`, which the game keeps until it is
//! changed back, adds to `*_Max_Base` and sends to clients itself. Impacts
//! of perks, buffs and gear change the adder by their own deltas, so the
//! loader adds and takes back only its own.

use std::sync::OnceLock;

use enshwick::Entity;
use enshwick::raw::{read_at, write_at};

use crate::host::Loader;
use crate::pinned;
use crate::pinned::Constant;

/// The pools with a maximum the loader raises.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pool {
    Health,
    Stamina,
    Mana,
}

/// One attribute set: its component type, the id it keeps first, and the
/// slots of its maximum and its adder.
#[derive(Debug, Clone, Copy)]
struct Set {
    ty: u32,
    root: u32,
    max: usize,
    adder: usize,
}

/// Where the writes go on the running build.
#[derive(Debug, Clone, Copy)]
struct Layout {
    data_offset: usize,
    flags: usize,
    count: usize,
    sets: [Set; 3],
}

use enshwick::__private::ATTRIBUTE_SET_INITIALISED as SET_INITIALISED;

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let constant = |name: Constant| loader.build.constant(name.id());
            let slot = |name: Constant| constant(name).and_then(|v| usize::try_from(v).ok());
            let set = |id: &str, [root, max, adder]: [Constant; 3]| {
                Some(Set {
                    ty: crate::catalogue::component_hash(loader, id)?,
                    root: u32::try_from(constant(root)?).ok()?,
                    max: slot(max)?,
                    adder: slot(adder)?,
                })
            };
            let layout = (|| {
                Some(Layout {
                    data_offset: slot(pinned::ecs::LEVEL_DATA_OFFSET)?,
                    flags: slot(pinned::ecs::LEVEL_FLAGS)?,
                    count: slot(pinned::ecs::ATTRIBUTE_COUNT)?,
                    sets: [
                        set(
                            crate::catalogue::ids::ATTRIBUTE_SET_HEALTH,
                            [
                                pinned::attribute::HEALTH,
                                pinned::attribute::HEALTH_MAX_SLOT,
                                pinned::attribute::HEALTH_ADDER_SLOT,
                            ],
                        )?,
                        set(
                            crate::catalogue::ids::ATTRIBUTE_SET_STAMINA,
                            [
                                pinned::attribute::STAMINA,
                                pinned::attribute::STAMINA_MAX_SLOT,
                                pinned::attribute::STAMINA_ADDER_SLOT,
                            ],
                        )?,
                        set(
                            crate::catalogue::ids::ATTRIBUTE_SET_MANA,
                            [
                                pinned::attribute::MANA,
                                pinned::attribute::MANA_MAX_SLOT,
                                pinned::attribute::MANA_ADDER_SLOT,
                            ],
                        )?,
                    ],
                })
            })();
            if layout.is_none() {
                crate::log!(
                    WARN,
                    "stat maxima off: this game build lacks the attribute layout; maximum health, stamina and mana stay the game's"
                );
            }
            layout
        })
        .as_ref()
}

/// Adds `delta` to `entity`'s maximum of `pool`, through its adder; false
/// when the entity has no such set or the build lacks the layout. On the
/// game thread, between steps.
pub(super) fn add_max(loader: &Loader, entity: Entity, pool: Pool, delta: i32) -> bool {
    let Some(layout) = layout(loader) else {
        return false;
    };
    let set = layout.sets[pool as usize];
    let Ok(data) = crate::writes::component_mut(loader, entity.0, set.ty) else {
        return false;
    };
    let data = data.cast::<u8>();
    // SAFETY: `writes::component_mut` handed out the entity's live attribute
    // set for writing on the game thread; the set starts with the `u32` id it
    // keeps.
    let root: u32 = unsafe { read_at(data, 0) };
    // SAFETY: the live attribute set from `writes::component_mut`, on the game
    // thread, holds a `u16` data offset at the build table's
    // `ecs.level_data_offset`.
    let offset = usize::from(unsafe { read_at::<u16>(data, layout.data_offset) });
    // SAFETY: the live attribute set from `writes::component_mut`, on the game
    // thread, holds a one-byte flag set at the build table's `ecs.level_flags`.
    let flags: u8 = unsafe { read_at(data, layout.flags) };
    // SAFETY: the live attribute set from `writes::component_mut`, on the game
    // thread, holds a one-byte value count at the build table's
    // `ecs.attribute_count`.
    let count = usize::from(unsafe { read_at::<u8>(data, layout.count) });
    if root != set.root || flags & SET_INITIALISED == 0 || set.max.max(set.adder) >= count {
        return false;
    }
    let add = |slot: usize| {
        let at = offset + slot * size_of::<i32>();
        // SAFETY: the live attribute set keeps `count` `i32` values from
        // `offset`, and the earlier check keeps `slot` below `count`.
        let old: i32 = unsafe { read_at(data, at) };
        // SAFETY: the same `i32` value slot inside the live attribute set,
        // handed out for writing by `writes::component_mut` on the game
        // thread, which holds an `i32` there.
        unsafe { write_at(data, at, old.saturating_add(delta)) };
    };
    // The game works the maximum out from its base and adder in the next
    // step, as for an impact's change; writing both keeps them in step until
    // then (checked on a test server, enemies' health and stamina, 1.0
    // build 1269579).
    add(set.adder);
    add(set.max);
    true
}
