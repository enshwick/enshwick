//! Props: the things players place from items, such as chests, beds and
//! torches, with the item each came from and where it stands.

use bytemuck::Zeroable;
use rustc_hash::FxHashMap;

use crate::Entity;
use crate::ItemId;
use crate::abi::{EntityPlace, Header, ItemData};

use super::{World, WorldError};

/// Where a thing stands and which way it faces: the game's `WorldTransform`
/// without its scale, which is 1 for every prop the game places.
///
/// # Examples
///
/// ```
/// use enshwick::world::WorldTransform;
///
/// let upright = WorldTransform { position: [10.0, 2.0, 4.5], orientation: [0.0, 0.0, 0.0, 1.0] };
/// assert_eq!(upright, WorldTransform::at([10.0, 2.0, 4.5]));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct WorldTransform {
    /// The position in metres, from the game's 32.32 fixed point.
    pub position: [f64; 3],
    /// The rotation as a unit quaternion, x, y, z then w, as the game
    /// stores it, with y up.
    pub orientation: [f32; 4],
}

impl WorldTransform {
    /// The orientation of a thing that is not turned.
    pub const UPRIGHT: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

    /// A thing at `position`, not turned.
    ///
    /// # Examples
    ///
    /// See [`WorldTransform`].
    #[must_use]
    pub const fn at(position: [f64; 3]) -> Self {
        Self {
            position,
            orientation: Self::UPRIGHT,
        }
    }
}

/// A placed prop, from [`World::props_in`] and [`Capture`](super::Capture).
///
/// # Examples
///
/// See [`World::props_in`].
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Prop {
    /// The prop's entity.
    pub entity: Entity,
    /// The item it was placed from, its `UsedItem`.
    pub item: ItemId,
    /// Where it stands and which way it faces, its `CurrentTransform`.
    pub transform: WorldTransform,
}

/// The box between two corners, each axis from the lower to the higher
/// value; `None` when a corner is not finite.
pub(crate) fn span(a: [f64; 3], b: [f64; 3]) -> Option<([f64; 3], [f64; 3])> {
    if a.iter().chain(&b).any(|axis| !axis.is_finite()) {
        return None;
    }
    let low = [0, 1, 2].map(|axis| a[axis].min(b[axis]));
    let high = [0, 1, 2].map(|axis| a[axis].max(b[axis]));
    Some((low, high))
}

impl World {
    /// The props whose position lies in the box between corners `a` and
    /// `b`, in metres, edges included, by entity: what a player placed
    /// there from an item, told apart by the template the item places,
    /// not the things a world comes with. Creatures placed from items
    /// (animals, pets, survivors) and flame altars are no props and are
    /// left out; the entity calls reach them. The engine walks every
    /// entity with a `UsedItem`, so call it now and then, not every step.
    ///
    /// # Errors
    ///
    /// [`WorldError::Refused`] for a corner that is not finite;
    /// [`WorldError::WrongThread`] off the server's game thread or inside a
    /// step; [`WorldError::Missing`] before a world runs or on a game build
    /// without the reads, the marks of creatures and altars included.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::world::{World, WorldError};
    ///
    /// fn torches(world: World, torch: enshwick::ItemId, at: [f64; 3]) -> Result<usize, WorldError> {
    ///     let [x, y, z] = at;
    ///     let props = world.props_in([x - 8.0, y - 4.0, z - 8.0], [x + 8.0, y + 4.0, z + 8.0])?;
    ///     Ok(props.iter().filter(|prop| prop.item == torch).count())
    /// }
    /// ```
    pub fn props_in(self, a: [f64; 3], b: [f64; 3]) -> Result<Vec<Prop>, WorldError> {
        let (low, high) = span(a, b).ok_or(WorldError::Refused)?;
        let orientation = self.0.orientation.ok_or(WorldError::Missing)?;
        let no_props = self.0.no_props.ok_or(WorldError::Missing)?;
        let mut templates = Templates::default();
        let mut props: Vec<Prop> = self
            .placed_with(self.0.used_item.hash())?
            .iter()
            .map(|place| (place.entity, crate::ecs::metres(place.position)))
            .filter(|(_, at)| (0..3).all(|axis| (low[axis]..=high[axis]).contains(&at[axis])))
            .filter_map(|(entity, position)| {
                if no_props
                    .iter()
                    .any(|&marks| self.component(entity, marks).is_some())
                {
                    return None;
                }
                let item = ItemId::new(self.used_item(entity)?);
                let placed = templates.placed_by(self, item)?;
                if self.template(entity) != Some(placed) {
                    return None;
                }
                let data = self.component(entity, self.0.transform)?;
                // SAFETY: `component` returned the entity's live
                // CurrentTransform, valid until the game thread moves on and
                // read at once; the loader's `orientation` offset lies inside
                // it, at its transform's 16 bytes of orientation.
                let orientation =
                    unsafe { crate::raw::read_at::<[f32; 4]>(data.cast(), orientation) };
                Some(Prop {
                    entity,
                    item,
                    transform: WorldTransform {
                        position,
                        orientation,
                    },
                })
            })
            .collect();
        if templates.found == 0 && templates.unknown > 0 {
            return Err(WorldError::Missing);
        }
        props.sort_unstable_by_key(|prop| prop.entity.0);
        Ok(props)
    }

    /// Every entity with a component of type `hash` and a place in the
    /// world, as the loader answers.
    pub(crate) fn placed_with(self, hash: u32) -> Result<Vec<EntityPlace>, WorldError> {
        let entities = self.0.host.api().entities;
        let mut capacity = 1024u32;
        loop {
            let mut places = vec![EntityPlace::default(); capacity as usize];
            let mut count = 0u32;
            let status = entities(
                self.0.host.handle(),
                hash,
                places.as_mut_ptr(),
                capacity,
                &raw mut count,
            );
            WorldError::from_status(status)?;
            if count <= capacity {
                places.truncate(count as usize);
                return Ok(places);
            }
            capacity = count;
        }
    }
}

/// The templates items place, looked up once per item during one query.
#[derive(Default)]
struct Templates {
    known: FxHashMap<ItemId, Option<[u8; 16]>>,
    /// Items the loader knew.
    found: usize,
    /// Items the loader did not know: all of them before the game first
    /// ran its inventories.
    unknown: usize,
}

impl Templates {
    /// The GUID of the template a player places from `item`; `None` for an
    /// item that places nothing or the loader does not know. The world
    /// resolves props only where the loader reads the template, so a zero
    /// GUID means the item places nothing.
    fn placed_by(&mut self, world: World, item: ItemId) -> Option<[u8; 16]> {
        *self.known.entry(item).or_insert_with(|| {
            let mut out = ItemData::zeroed();
            out.header = Header::of::<ItemData>();
            let host = world.0.host;
            if (host.api().item_info)(host.handle(), item, &raw mut out).is_ok() {
                self.found += 1;
                Some(out.placed_entity).filter(|guid| *guid != [0; 16])
            } else {
                self.unknown += 1;
                None
            }
        })
    }
}
