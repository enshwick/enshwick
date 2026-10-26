//! Copy staffs from before charges get their charges at once: in the
//! dedicated server and in a game a client hosts, every copy staff without an entity of its own, in a player's
//! inventory or a container, is renewed full, when the world loads, when a
//! player joins and every minute for containers loaded since.

use std::ops::ControlFlow;
use std::time::Duration;

use enshwick::ecs::Entity;
use enshwick::events::{PlayerJoined, WorldLoaded};
use enshwick::inventory::{Fill, Inventories};
use enshwick::world::World;
use enshwick::{Load, Off};

/// How often containers are looked through for old staffs.
const SWEEP: Duration = Duration::from_secs(60);

/// Renews the old copy staffs in `root`; logs what it asked for.
fn renew(inventories: &Inventories, root: Entity, staffs: &[u32]) {
    let asked = inventories.renew(root, staffs, Fill::FULL, move |outcome| {
        enshwick::info!(
            "old copy staffs in {} renewed with full charges: {:?}",
            root.0,
            outcome.done
        );
    });
    if asked.is_some() {
        enshwick::debug!("old copy staffs in {}: renewal asked", root.0);
    }
}

/// Every player and container of the world.
fn holders(world: &World, containers: Option<enshwick::world::ComponentType>) -> Vec<Entity> {
    let mut roots: Vec<Entity> = world.players().iter().map(|p| p.entity).collect();
    if let Some(ty) = containers {
        roots.extend(world.entities(ty).iter().map(|placed| placed.entity));
    }
    roots
}

/// Links the renewals: they run in the dedicated server and in a game a
/// client hosts, solo play included.
pub(crate) fn link(load: &mut Load<'_>) -> Result<(), Off> {
    let inventories = load.inventories()?;
    let world = load.world()?;
    let scheduler = load.scheduler()?;
    let containers = world.component_type(load, "keen::ecs::InventorySetup").ok();
    let staffs = crate::staffs::known_ids();
    let sweep = {
        let staffs = staffs.clone();
        move || {
            for root in holders(&world, containers) {
                renew(&inventories, root, &staffs);
            }
        }
    };
    let loaded = sweep.clone();
    load.on::<WorldLoaded>(move |()| loaded())?;
    scheduler.every(SWEEP, move || {
        sweep();
        ControlFlow::Continue(())
    });
    load.on::<PlayerJoined>(move |joined| {
        renew(&inventories, Entity(joined.entity), &staffs);
    })?;
    Ok(())
}
