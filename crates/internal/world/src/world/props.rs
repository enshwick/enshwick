//! Props: the things players place from items, such as chests, beds and
//! torches, with the item each came from and where it stands.

use rustc_hash::FxHashMap;

use crate::Entity;
use crate::ItemId;
use crate::content::Guid;

use super::{World, WorldError};

/// Declared local placement corners in metres, before a prop turns.
/// This is item metadata, not a complete physical or permission footprint.
///
/// # Examples
///
/// ```
/// let bounds = enshwick::world::PropPlacementBox::new([-1.0, 0.0, -1.0], [1.0, 2.0, 1.0]);
/// assert_eq!(bounds.max[1], 2.0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct PropPlacementBox {
    /// The declared minimum corner, which may lie above `max` on some axes.
    pub min: [f32; 3],
    /// The declared maximum corner, above `min` on at least one axis.
    pub max: [f32; 3],
}

impl PropPlacementBox {
    /// A `PropPlacementBox` of its parts.
    ///
    /// # Examples
    ///
    /// See [`PropPlacementBox`].
    #[must_use]
    pub const fn new(min: [f32; 3], max: [f32; 3]) -> Self {
        Self { min, max }
    }
}

/// An item's prop template and optional declared local placement box.
///
/// These are copied metadata, not a complete physical footprint or proof of
/// permission, collision, persistence, label ownership or safe recovery.
///
/// # Examples
///
/// ```
/// fn inspect(world: enshwick::world::World, item: enshwick::ItemId) -> Result<(), enshwick::WorldError> {
///     let info = world.prop_placement_info(item)?;
///     assert_eq!(info.item, item);
///     Ok(())
/// }
/// ```
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct PropPlacementInfo {
    /// The queried item.
    pub item: ItemId,
    /// The template it places.
    pub template: Guid,
    /// The resolver's local corners in metres, before rotation. None means
    /// absent, invalid or unavailable reads, never a known empty footprint.
    /// The loader's fallback permission box is not returned here.
    pub local_box: Option<PropPlacementBox>,
}

/// Where a thing stands and which way it faces: the game's `WorldTransform`
/// without its scale, which is 1 for every prop the game places.
///
/// # Examples
///
/// ```
/// use enshwick::world::WorldTransform;
///
/// let upright = WorldTransform::new([10.0, 2.0, 4.5], [0.0, 0.0, 0.0, 1.0]);
/// assert_eq!(upright, WorldTransform::at([10.0, 2.0, 4.5]));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize, crate::Script)]
#[non_exhaustive]
pub struct WorldTransform {
    /// The position in metres, from the game's 32.32 fixed point.
    pub position: [f64; 3],
    /// The rotation as a unit quaternion, x, y, z then w, as the game
    /// stores it, with y up.
    pub orientation: [f32; 4],
}

impl WorldTransform {
    /// A `WorldTransform` of its parts.
    ///
    /// # Examples
    ///
    /// See [`WorldTransform`].
    #[must_use]
    pub const fn new(position: [f64; 3], orientation: [f32; 4]) -> Self {
        Self {
            position,
            orientation,
        }
    }
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
#[derive(Clone, Copy, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct Prop {
    /// The prop's entity.
    pub entity: Entity,
    /// The item it was placed from, its `UsedItem`.
    pub item: ItemId,
    /// Where it stands and which way it faces, its `CurrentTransform`.
    pub transform: WorldTransform,
}

impl Prop {
    /// A `Prop` of its parts.
    ///
    /// # Examples
    ///
    /// See [`Prop`].
    #[must_use]
    pub const fn new(entity: Entity, item: ItemId, transform: WorldTransform) -> Self {
        Self {
            entity,
            item,
            transform,
        }
    }
}

/// A prop as a [`Capture`](super::Capture) keeps it: the item to place and
/// where, without the entity, whose id lasts one session.
///
/// # Examples
///
/// ```
/// use enshwick::ItemId;
/// use enshwick::world::{CapturedProp, Prop, WorldTransform};
///
/// let prop = Prop::new(enshwick::Entity::from_raw(41).ok_or("no entity")?, ItemId::new(7), WorldTransform::at([1.0, 2.0, 3.0]));
/// assert_eq!(CapturedProp::from(prop).item, ItemId::new(7));
/// # Ok::<(), &str>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize, crate::Script)]
#[non_exhaustive]
pub struct CapturedProp {
    /// The item it was placed from, its `UsedItem`.
    pub item: ItemId,
    /// Where it stands and which way it faces, counted from the capture's
    /// corner.
    pub transform: WorldTransform,
}

impl CapturedProp {
    /// A `CapturedProp` of its parts.
    ///
    /// # Examples
    ///
    /// See [`CapturedProp`].
    #[must_use]
    pub const fn new(item: ItemId, transform: WorldTransform) -> Self {
        Self { item, transform }
    }
}

impl From<Prop> for CapturedProp {
    fn from(prop: Prop) -> Self {
        Self {
            item: prop.item,
            transform: prop.transform,
        }
    }
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

#[crate::script(props)]
impl World {
    /// Copies the prop template and optional local placement box of `item`,
    /// without spawning an entity or committing a transaction. Any thread;
    /// the loader pins the server's item registry for the read.
    ///
    /// # Errors
    ///
    /// `Refused` for an unknown item or one that places nothing; `NotAProp` for
    /// creatures and flame altars; `Unavailable` without the running server,
    /// registry or required reads. Missing box reads return `local_box=None`,
    /// not a fallback footprint or a placement permission.
    ///
    /// # Examples
    ///
    /// See [`PropPlacementInfo`].
    #[script]
    pub fn prop_placement_info(self, item: ItemId) -> Result<PropPlacementInfo, WorldError> {
        let mut info = enshwick_abi::headed!(crate::abi::PropPlacementInfo {
            template: Guid::NIL,
            min: [0.0; 3],
            max: [0.0; 3],
            flags: 0,
        });
        let status =
            (self.0.host.api().prop_placement_info)(self.0.host.handle(), item, &raw mut info);
        WorldError::from_status(status)?;
        Ok(PropPlacementInfo {
            item,
            template: info.template,
            local_box: (info.flags & crate::abi::PropPlacementInfo::HAS_LOCAL_BOX != 0).then_some(
                PropPlacementBox {
                    min: info.min,
                    max: info.max,
                },
            ),
        })
    }

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
    /// step; [`WorldError::Unavailable`] before a world runs or on a game build
    /// without the reads, the marks of creatures and altars included.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::WorldError;
    /// use enshwick::world::World;
    ///
    /// fn torches(world: World, torch: enshwick::ItemId, at: [f64; 3]) -> Result<usize, WorldError> {
    ///     let [x, y, z] = at;
    ///     let props = world.props_in([x - 8.0, y - 4.0, z - 8.0], [x + 8.0, y + 4.0, z + 8.0])?;
    ///     Ok(props.iter().filter(|prop| prop.item == torch).count())
    /// }
    /// ```
    #[script]
    pub fn props_in(self, a: [f64; 3], b: [f64; 3]) -> Result<Vec<Prop>, WorldError> {
        let (low, high) = span(a, b).ok_or(WorldError::Refused)?;
        let orientation = self.0.orientation.ok_or(WorldError::Unavailable)?;
        let no_props = self.0.no_props.ok_or(WorldError::Unavailable)?;
        let mut templates = Templates::default();
        let mut props: Vec<Prop> = self
            .placed_with(self.0.used_item.hash())?
            .iter()
            .map(|place| (place.entity, place.position))
            .filter(|(_, at)| (0..3).all(|axis| (low[axis]..=high[axis]).contains(&at[axis])))
            .filter_map(|(entity, position)| {
                if no_props
                    .iter()
                    .any(|&marks| self.component_at(entity, marks).is_some())
                {
                    return None;
                }
                let item = self.used_item(entity)?;
                let placed = templates.placed_by(self, item)?;
                if self.template(entity) != Some(placed) {
                    return None;
                }
                let data = self.component_at(entity, self.0.transform)?;
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
            return Err(WorldError::Unavailable);
        }
        props.sort_unstable_by_key(|prop| prop.entity.get());
        Ok(props)
    }
}

/// The templates items place, looked up once per item during one query.
#[derive(Default)]
struct Templates {
    known: FxHashMap<ItemId, Option<Guid>>,
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
    fn placed_by(&mut self, world: World, item: ItemId) -> Option<Guid> {
        *self.known.entry(item).or_insert_with(|| {
            if let Some(data) = enshwick_content::__internal::item_data(&world.0.host, item) {
                self.found += 1;
                Some(data.placed_entity).filter(|guid| *guid != Guid::NIL)
            } else {
                self.unknown += 1;
                None
            }
        })
    }
}
