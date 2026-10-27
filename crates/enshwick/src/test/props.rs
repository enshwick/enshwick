//! The fake's props: what stands in its world, the state each holds, and how transactions change them.

use super::{FakeHost, FakeTransaction, Installed, table};
use crate::ItemId;
use crate::abi::{PropRemove, Status, TransactionInfo};
use crate::ecs::Entity;
use crate::test::FakeChange;
use crate::world::HeldState;

/// Half the side of the box the fake checks a player's rights for around a
/// prop, in metres, as the loader does for an item without a placement
/// box.
const REACH: f64 = 2.0;
/// The first entity id the fake hands a placed prop.
const FIRST_ENTITY: u32 = 1000;
/// Which way a prop faces when the test does not say.
const STILL: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

/// A prop in the fake's world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FakeProp {
    pub(crate) entity: u32,
    pub(crate) item: u32,
    pub(crate) at: [f64; 3],
    pub(crate) orientation: [f32; 4],
    /// The kinds of state it holds, as `TransactionInfo::HOLDS_` bits.
    pub(crate) holds: u32,
    /// Whether its start state changed since it was placed: a logic
    /// trigger's settings, a plant's growth.
    pub(crate) settings_changed: bool,
}

impl FakeHost {
    /// A prop standing in the fake's world: `entity`, placed from `item`, at
    /// `at` in metres, holding no state until
    /// [`Installed::set_prop_state`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ItemId;
    /// use enshwick::ecs::Entity;
    ///
    /// let loader = enshwick::test::FakeHost::new().prop(Entity(40), ItemId::new(30), [50.0, 40.0, 50.0]).install();
    /// assert_eq!(loader.props().len(), 1);
    /// ```
    #[must_use]
    pub fn prop(mut self, entity: Entity, item: ItemId, at: [f64; 3]) -> Self {
        self.props.push(FakeProp {
            entity: entity.0,
            item: item.get(),
            at,
            orientation: STILL,
            holds: 0,
            settings_changed: false,
        });
        self
    }

    /// Makes the props of item `item` logic triggers, such as the logic
    /// trigger prop: removing one is refused, and an undo takes one it
    /// placed away only while its settings are unchanged
    /// ([`Installed::change_trigger_settings`]).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ItemId;
    /// use enshwick::ecs::Entity;
    ///
    /// let _loader = enshwick::test::FakeHost::new().item_triggers(30).prop(Entity(40), ItemId::new(30), [50.0; 3]).install();
    /// ```
    #[must_use]
    pub fn item_triggers(mut self, item: u32) -> Self {
        self.trigger_items.push(item);
        self
    }

    /// Makes the props of item `item` plants: removing one is refused, and
    /// an undo takes one it placed away only while it has not grown
    /// ([`Installed::grow_plant`]).
    ///
    /// # Examples
    ///
    /// ```
    /// let _loader = enshwick::test::FakeHost::new().item_grows(30).install();
    /// ```
    #[must_use]
    pub fn item_grows(mut self, item: u32) -> Self {
        self.growing_items.push(item);
        self
    }

    /// Makes item `item` place something that is no prop, as an animal, a
    /// pet, a survivor or a flame altar is: placing it is refused at
    /// commit, and removing one of its props is refused, discarding state
    /// or not.
    ///
    /// # Examples
    ///
    /// ```
    /// let _loader = enshwick::test::FakeHost::new().item_not_a_prop(30).install();
    /// ```
    #[must_use]
    pub fn item_not_a_prop(mut self, item: u32) -> Self {
        self.no_prop_items.push(item);
        self
    }

    /// Gives item `item` a placement box from `min` to `max` in metres
    /// around its prop's position, before it turns with the prop; the fake
    /// checks a player's rights for that box, turned, as the loader does.
    ///
    /// # Examples
    ///
    /// ```
    /// let _loader = enshwick::test::FakeHost::new().placement_box(30, [-1.0, 0.0, -0.5], [1.0, 2.0, 0.5]).install();
    /// ```
    #[must_use]
    pub fn placement_box(mut self, item: u32, min: [f32; 3], max: [f32; 3]) -> Self {
        self.placement_boxes.push((item, min, max));
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The props in the fake's world, by entity, with their item and
    /// position.
    ///
    /// # Examples
    ///
    /// See [`FakeHost::prop`].
    #[must_use]
    pub fn props(&self) -> Vec<(Entity, ItemId, [f64; 3])> {
        table::with(Vec::new(), |s| {
            s.host
                .props
                .iter()
                .map(|prop| (Entity(prop.entity), ItemId::new(prop.item), prop.at))
                .collect()
        })
    }

    /// Changes the logic trigger settings of the prop `entity`, as a player
    /// does.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ItemId;
    /// use enshwick::ecs::Entity;
    ///
    /// let loader = enshwick::test::FakeHost::new().item_triggers(30).prop(Entity(40), ItemId::new(30), [50.0; 3]).install();
    /// loader.change_trigger_settings(Entity(40));
    /// ```
    pub fn change_trigger_settings(&self, entity: Entity) {
        change_start(entity);
    }

    /// Grows the plant `entity` a stage, as time does.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ItemId;
    /// use enshwick::ecs::Entity;
    ///
    /// let loader = enshwick::test::FakeHost::new().item_grows(30).prop(Entity(40), ItemId::new(30), [50.0; 3]).install();
    /// loader.grow_plant(Entity(40));
    /// ```
    pub fn grow_plant(&self, entity: Entity) {
        change_start(entity);
    }

    /// Makes the prop `entity` hold `kinds` of state a player would lose,
    /// none for an empty slice: a chest given items, a bed given a
    /// survivor.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::ItemId;
    /// use enshwick::ecs::Entity;
    /// use enshwick::world::HeldState;
    ///
    /// let loader = enshwick::test::FakeHost::new().prop(Entity(40), ItemId::new(30), [50.0; 3]).install();
    /// loader.set_prop_state(Entity(40), &[HeldState::Inventory]);
    /// ```
    pub fn set_prop_state(&self, entity: Entity, kinds: &[HeldState]) {
        let holds = kinds.iter().fold(0, |bits, kind| bits | kind.bit());
        table::with((), |s| {
            if let Some(prop) = s.host.props.iter_mut().find(|prop| prop.entity == entity.0) {
                prop.holds = holds;
            }
        });
    }
}

/// Marks the start state of the prop `entity` changed.
fn change_start(entity: Entity) {
    table::with((), |s| {
        if let Some(prop) = s.host.props.iter_mut().find(|prop| prop.entity == entity.0) {
            prop.settings_changed = true;
        }
    });
}

/// `vector` turned by the unit quaternion `rotation` (x, y, z, w).
fn turn(rotation: [f32; 4], vector: [f64; 3]) -> [f64; 3] {
    let [qx, qy, qz, real] = rotation.map(f64::from);
    let axis = [qx, qy, qz];
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let twice = cross(axis, vector).map(|part| 2.0 * part);
    let again = cross(axis, twice);
    [0, 1, 2].map(|i| vector[i] + real * twice[i] + again[i])
}

/// The box the fake checks a player's rights for around a prop of `item`
/// at `at` facing `orientation`: its placement box turned with it, else
/// `REACH` each way.
pub(crate) fn reach(
    host: &FakeHost,
    item: u32,
    at: [f64; 3],
    orientation: [f32; 4],
) -> ([f64; 3], [f64; 3]) {
    let (low, high) = match host.placement_boxes.iter().find(|(of, ..)| *of == item) {
        Some(&(_, min, max)) => {
            let mut low = [f64::INFINITY; 3];
            let mut high = [f64::NEG_INFINITY; 3];
            for corner in 0..8 {
                let pick = |axis: usize| {
                    if corner >> axis & 1 == 0 {
                        f64::from(min[axis])
                    } else {
                        f64::from(max[axis])
                    }
                };
                let turned = turn(orientation, [pick(0), pick(1), pick(2)]);
                for axis in 0..3 {
                    low[axis] = low[axis].min(turned[axis]);
                    high[axis] = high[axis].max(turned[axis]);
                }
            }
            (low, high)
        }
        None => ([-REACH; 3], [REACH; 3]),
    };
    (
        [0, 1, 2].map(|axis| (at[axis] + low[axis]).max(0.0)),
        [0, 1, 2].map(|axis| at[axis] + high[axis]),
    )
}

/// The start state bits of `prop`, as the loader counts them: for any
/// logic trigger or plant when `unknown` (one the transaction did not
/// place), else for one whose start state changed since it was placed.
fn start_bits(host: &FakeHost, prop: &FakeProp, unknown: bool) -> u32 {
    if !unknown && !prop.settings_changed {
        return 0;
    }
    let mut bits = 0;
    if host.trigger_items.contains(&prop.item) {
        bits |= TransactionInfo::HOLDS_LOGIC_GRAPH_TRIGGER;
    }
    if host.growing_items.contains(&prop.item) {
        bits |= TransactionInfo::HOLDS_GROWTH;
    }
    bits
}

/// The next entity id the fake hands out.
fn next_entity(host: &FakeHost) -> u32 {
    host.props
        .iter()
        .map(|prop| prop.entity + 1)
        .max()
        .unwrap_or(FIRST_ENTITY)
        .max(FIRST_ENTITY)
}

/// Writes `transaction`'s prop changes, as the loader's step does: every
/// removal is checked before anything is written. The reason and the
/// kinds of state that stop it.
pub(crate) fn apply(
    host: &mut FakeHost,
    transaction: &mut FakeTransaction,
) -> Result<(), (Status, u32)> {
    for change in &transaction.changes {
        if let FakeChange::RemoveProp(remove) = change {
            let prop = host
                .props
                .iter()
                .find(|prop| prop.entity == remove.entity)
                .ok_or((Status::NOT_A_PROP, 0))?;
            if host.no_prop_items.contains(&prop.item) {
                return Err((Status::NOT_A_PROP, 0));
            }
            let holds = prop.holds | start_bits(host, prop, true);
            if holds != 0 && remove.flags & PropRemove::DISCARD_STATE == 0 {
                return Err((Status::REFUSED, holds));
            }
        }
    }
    for change in &transaction.changes {
        match change {
            FakeChange::PlaceProp(place) => {
                let entity = next_entity(host);
                host.props.push(FakeProp {
                    entity,
                    item: place.item,
                    at: place.position,
                    orientation: place.orientation,
                    holds: 0,
                    settings_changed: false,
                });
                transaction.created.push(entity);
            }
            FakeChange::RemoveProp(remove) => {
                if let Some(index) = host
                    .props
                    .iter()
                    .position(|prop| prop.entity == remove.entity)
                {
                    let mut prop = host.props.remove(index);
                    prop.holds |= start_bits(host, &prop, true);
                    transaction.removed.push(prop);
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Takes back `transaction`'s prop changes, as the loader's undo does: a
/// placed prop that now holds state stays, a removed prop that held state
/// does not come back, nor one whose place a like prop has taken. The
/// reason and the kinds of state that stop it.
pub(crate) fn undo(
    host: &mut FakeHost,
    transaction: &mut FakeTransaction,
) -> Result<(), (Status, u32)> {
    for entity in &transaction.created {
        let prop = host
            .props
            .iter()
            .find(|prop| prop.entity == *entity)
            .ok_or((Status::REFUSED, 0))?;
        let holds = prop.holds | start_bits(host, prop, false);
        if holds != 0 {
            return Err((Status::REFUSED, holds));
        }
    }
    for removed in &transaction.removed {
        if removed.holds != 0 {
            return Err((Status::REFUSED, removed.holds));
        }
        let taken = host.props.iter().any(|prop| {
            prop.item == removed.item && crate::ecs::fixed(prop.at) == crate::ecs::fixed(removed.at)
        });
        if taken {
            return Err((Status::REFUSED, 0));
        }
    }
    host.props
        .retain(|prop| !transaction.created.contains(&prop.entity));
    transaction.created.clear();
    for prop in transaction.removed.drain(..) {
        let entity = next_entity(host);
        host.props.push(FakeProp { entity, ..prop });
    }
    Ok(())
}
