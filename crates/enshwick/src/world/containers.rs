//! The world's containers by kind: every entity that holds items, told
//! apart by a component each kind carries.

use core::ops::BitOr;

use crate::Load;
use crate::ecs::Entity;
use crate::engine_ids as ids;
use crate::world::{ComponentType, Placed, World};

/// Kinds of entities that hold items, for [`World::containers`]; combine
/// them with `|`. The default is [`ContainerKinds::PLACED`].
///
/// # Examples
///
/// ```
/// use enshwick::world::ContainerKinds;
///
/// let kinds = ContainerKinds::PLACED | ContainerKinds::CHARACTERS;
/// assert!(kinds.contains(ContainerKinds::CHARACTERS));
/// assert!(!kinds.contains(ContainerKinds::DROPS));
/// assert_eq!(ContainerKinds::default(), ContainerKinds::PLACED);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ContainerKinds(u32);

impl ContainerKinds {
    /// What players build: storage chests, showcases and crafting
    /// stations.
    pub const PLACED: Self = Self(1);
    /// The world's own loot chests.
    pub const LOOT: Self = Self(1 << 1);
    /// Loot that goes once emptied: enemies' corpses and drops, players'
    /// tombstones, trash heaps.
    pub const DROPS: Self = Self(1 << 2);
    /// Players' characters.
    pub const CHARACTERS: Self = Self(1 << 3);
    /// Non-player characters.
    pub const NPCS: Self = Self(1 << 4);
    /// Animals.
    pub const ANIMALS: Self = Self(1 << 5);
    /// Plants and other nodes that hold what they yield.
    pub const PLANTS: Self = Self(1 << 6);
    /// Whatever holds items and is none of the kinds above, such as a
    /// kind a later version of the API tells apart.
    pub const OTHER: Self = Self(1 << 31);
    /// Every kind, those a later version adds included.
    pub const ALL: Self = Self(u32::MAX);

    /// Whether every kind of `other` is in `self`.
    ///
    /// # Examples
    ///
    /// See [`ContainerKinds`].
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl Default for ContainerKinds {
    fn default() -> Self {
        Self::PLACED
    }
}

impl BitOr for ContainerKinds {
    type Output = Self;

    fn bitor(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Each kind with the id of the component that marks it, tried in this
/// order; a holder with none of them is [`ContainerKinds::OTHER`]. The
/// order matters: animals and summoned NPCs keep the item they came from as
/// placed props do, and placed storage carries the world chests' loot fill.
const MARKERS: [(ContainerKinds, &str); 7] = [
    (ContainerKinds::CHARACTERS, ids::PLAYER_INPUT),
    (ContainerKinds::NPCS, ids::NPC),
    (ContainerKinds::ANIMALS, ids::ANIMAL),
    (ContainerKinds::PLACED, ids::USED_ITEM),
    (ContainerKinds::DROPS, ids::DESTROY_ON_LOOT),
    (ContainerKinds::LOOT, ids::LOOT_FILL),
    (ContainerKinds::PLANTS, ids::RESOURCE_NODE),
];

/// The component every holder has and the markers of its kinds, resolved
/// with the world.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Containers {
    setup: ComponentType,
    markers: [(ContainerKinds, ComponentType); MARKERS.len()],
}

impl Containers {
    /// The holders' component and every marker; `None` on a build that
    /// lacks any of them, since a holder whose marker is missing would fall
    /// to a later kind, a summoned NPC to the built ones.
    pub(crate) fn resolve(load: &Load<'_>, world: &World) -> Option<Self> {
        let resolve = |id| {
            world
                .component_type(load, id)
                .inspect_err(|off| crate::debug!("container queries off: {off}"))
                .ok()
        };
        let setup = resolve(ids::INVENTORY_SETUP)?;
        let mut markers = [(ContainerKinds::OTHER, ComponentType(0)); MARKERS.len()];
        for (slot, (kind, id)) in markers.iter_mut().zip(MARKERS) {
            *slot = (kind, resolve(id)?);
        }
        Some(Self { setup, markers })
    }

    /// The kind of the holder `entity`.
    fn kind(&self, world: &World, entity: Entity) -> ContainerKinds {
        self.markers
            .iter()
            .find(|&&(_, ty)| world.component(entity, ty).is_some())
            .map_or(ContainerKinds::OTHER, |&(kind, _)| kind)
    }

    /// The holders of a kind in `kinds`.
    pub(crate) fn of(&self, world: &World, kinds: ContainerKinds) -> Vec<Placed> {
        let mut found = world.entities(self.setup);
        found.retain(|placed| kinds.contains(self.kind(world, placed.entity)));
        found
    }
}
