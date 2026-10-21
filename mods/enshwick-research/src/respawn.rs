//! R20: where a player comes back after dying. The server keeps the chosen
//! spawn target in slot 9 of the player's `SelectedTargets`, a transform
//! and an entity; every five seconds this logs each player's slot when it
//! changes, with the player's own position.

use std::ops::ControlFlow;
use std::sync::Mutex;
use std::time::Duration;

use enshwick_loader_api::world::{ComponentType, World};
use enshwick_loader_api::{Load, Off};
use rustc_hash::FxHashMap;

use crate::memory;

/// The target slot of the spawn point; build 1269579.
const SPAWN_SLOT: usize = 9;
/// How often the slots are read.
const PERIOD: Duration = Duration::from_secs(5);

/// Where slot 9 keeps its position and entity inside `SelectedTargets`.
#[derive(Debug, Clone, Copy)]
struct Slot {
    ty: ComponentType,
    position: usize,
    entity: usize,
}

/// A slot as read: position in whole metres and entity.
type Seen = ([i64; 3], u32);

/// The last slot seen per account.
static LAST: Mutex<Option<FxHashMap<u64, Seen>>> = Mutex::new(None);

/// Starts the reads on the server's game thread.
pub(crate) fn link(load: &mut Load<'_>, world: World) -> Result<(), Off> {
    let offset = |ty: &str, field: &str| {
        load.field_offset(ty, field).and_then(|o| {
            usize::try_from(o).map_err(|_| Off::new(format!("{ty}.{field}: out of range")))
        })
    };
    let target = usize::try_from(load.type_size("keen::ecs::TargetInfo")?)
        .map_err(|_| Off::new("TargetInfo: out of range"))?;
    let base = offset("keen::ecs::SelectedTargets", "targets")? + SPAWN_SLOT * target;
    let slot = Slot {
        ty: world.component_type(load, "keen::ecs::SelectedTargets")?,
        position: base + offset("keen::ecs::TargetInfo", "targetTransform")?,
        entity: base + offset("keen::ecs::TargetInfo", "targetEntityId")?,
    };
    load.scheduler()?.every(PERIOD, move || {
        sweep(&world, slot);
        ControlFlow::Continue(())
    });
    Ok(())
}

fn sweep(world: &World, slot: Slot) {
    let mut last = LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let last = last.get_or_insert_with(FxHashMap::default);
    for player in world.players() {
        let Some(data) = world.component(player.entity, slot.ty) else {
            continue;
        };
        // The transform starts with its position, three 32.32 fixed-point
        // `i64`; the entity id is a `u32`.
        let data = data.addr();
        let mut position = [0u8; 24];
        let mut entity = [0u8; 4];
        if memory::read(data + slot.position, &mut position) < position.len()
            || memory::read(data + slot.entity, &mut entity) < entity.len()
        {
            continue;
        }
        let entity = u32::from_le_bytes(entity);
        let mut metres = [0i64; 3];
        for (m, bytes) in metres.iter_mut().zip(position.as_chunks::<8>().0) {
            *m = i64::from_le_bytes(*bytes) >> 32;
        }
        if last.get(&player.key) == Some(&(metres, entity)) {
            continue;
        }
        last.insert(player.key, (metres, entity));
        enshwick_loader_api::info!(
            "R20 spawn target of {:?}: entity {entity} at {metres:?} m; the player stands at {:?}",
            player.name,
            world.position(player.entity)
        );
    }
}
