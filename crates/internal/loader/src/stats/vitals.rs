//! Extra maximum health, stamina and mana, given the way gear gives them:
//! a change of the set's `*_Max_Adder`, which the game keeps until it is
//! changed back, adds to `*_Max_Base` and sends to clients itself. Impacts
//! of perks, buffs and gear change the adder by their own deltas, so the
//! loader adds and takes back only its own.

use std::sync::OnceLock;

use crate::host::Loader;

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

/// The attribute set's flag bit that marks it initialised.
const SET_INITIALISED: u8 = 1;

static LAYOUT: OnceLock<Option<Layout>> = OnceLock::new();

fn layout(loader: &Loader) -> Option<&'static Layout> {
    LAYOUT
        .get_or_init(|| {
            let constant = |name: &str| loader.build.constant(name);
            let slot = |name: &str| constant(name).and_then(|v| usize::try_from(v).ok());
            let set = |ty: &str, pool: &str| {
                let info = loader.reflection.type_info(ty).ok()?;
                Some(Set {
                    ty: crate::world::type_hash(info.va())?,
                    root: u32::try_from(constant(&format!("attribute.{pool}"))?).ok()?,
                    max: slot(&format!("attribute.{pool}_max_slot"))?,
                    adder: slot(&format!("attribute.{pool}_adder_slot"))?,
                })
            };
            let layout = (|| {
                Some(Layout {
                    data_offset: slot("ecs.level_data_offset")?,
                    flags: slot("ecs.level_flags")?,
                    count: slot("ecs.attribute_count")?,
                    sets: [
                        set("keen::ecs::Health", "health")?,
                        set("keen::ecs::Stamina", "stamina")?,
                        set("keen::ecs::Mana", "mana")?,
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
pub(super) fn add_max(loader: &Loader, entity: u32, pool: Pool, delta: i32) -> bool {
    let Some(layout) = layout(loader) else {
        return false;
    };
    let set = layout.sets[pool as usize];
    let Ok(data) = crate::writes::component_mut(loader, entity, set.ty) else {
        return false;
    };
    let data = data.cast::<u8>();
    let read = |at: usize| data.wrapping_add(at);
    // SAFETY: the entity's live attribute set, handed out for writing on the
    // game thread, starts with the id it keeps and holds its header at the
    // build's offsets.
    let root = unsafe { read(0).cast::<u32>().read_unaligned() };
    // SAFETY: as above.
    let offset = usize::from(unsafe { read(layout.data_offset).cast::<u16>().read_unaligned() });
    // SAFETY: as above.
    let flags = unsafe { read(layout.flags).read() };
    // SAFETY: as above.
    let count = usize::from(unsafe { read(layout.count).read() });
    if root != set.root || flags & SET_INITIALISED == 0 || set.max.max(set.adder) >= count {
        return false;
    }
    let add = |slot: usize| {
        let at = read(offset + slot * size_of::<i32>());
        // SAFETY: the engine keeps `count` values from `offset`, and `slot`
        // is below it.
        let old = i32::from_le_bytes(unsafe { at.cast::<[u8; 4]>().read() });
        // SAFETY: as above; the set was handed out for writing.
        unsafe {
            at.cast::<[u8; 4]>()
                .write(old.saturating_add(delta).to_le_bytes());
        };
    };
    // The game works the maximum out from its base and adder in the next
    // step, as for an impact's change; writing both keeps them in step until
    // then (checked on a test server, enemies' health and stamina, 1.0
    // build 1269579).
    add(set.adder);
    add(set.max);
    true
}
