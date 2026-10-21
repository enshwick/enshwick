//! Copy staffs from before charges get their charges at once: on the
//! server, every copy staff without an entity of its own, in a player's
//! inventory or a container, is renewed full, when the world loads, when a
//! player joins and every minute for containers loaded since.

use std::ops::ControlFlow;
use std::time::Duration;

use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::events::{PlayerJoined, WorldLoaded};
use enshwick_loader_api::inventory::{Fill, Inventories};
use enshwick_loader_api::world::World;
use enshwick_loader_api::{Load, Off};

/// How often containers are looked through for old staffs.
const SWEEP: Duration = Duration::from_secs(60);

/// Renews the old copy staffs in `root`; logs what it asked for.
fn renew(inventories: &Inventories, root: Entity, staffs: &[u32]) {
    let asked = inventories.renew(root, staffs, Fill::FULL, move |outcome| {
        enshwick_loader_api::info!(
            "old copy staffs in {} renewed with full charges: {:?}",
            root.0,
            outcome.done
        );
    });
    if asked.is_some() {
        enshwick_loader_api::debug!("old copy staffs in {}: renewal asked", root.0);
    }
}

/// Every player and container of the world.
fn holders(
    world: &World,
    containers: Option<enshwick_loader_api::world::ComponentType>,
) -> Vec<Entity> {
    let mut roots: Vec<Entity> = world.players().iter().map(|p| p.entity).collect();
    if let Some(ty) = containers {
        roots.extend(world.entities(ty).iter().map(|placed| placed.entity));
    }
    roots
}

/// Links the renewals on the server.
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
