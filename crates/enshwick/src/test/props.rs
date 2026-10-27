//! The fake's props: what stands in its world, the state each holds, and how transactions change them.

use super::{FakeHost, FakeTransaction, Installed, table};
use crate::ItemId;
use crate::abi::{PropRemove, Status};
use crate::ecs::Entity;
use crate::test::FakeChange;
use crate::world::HeldState;

/// Half the side of the box the fake checks a player's rights for around a
/// prop, in metres, as the loader without an item's placement box.
const REACH: f64 = 2.0;
/// The first entity id the fake hands a placed prop.
const FIRST_ENTITY: u32 = 1000;

/// A prop in the fake's world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct FakeProp {
    pub(crate) entity: u32,
    pub(crate) item: u32,
    pub(crate) at: [f64; 3],
    /// The kinds of state it holds, as `TransactionInfo::HOLDS_` bits.
    pub(crate) holds: u32,
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
            holds: 0,
        });
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

    /// Makes the prop `entity` hold `kinds` of state a player would lose,
    /// none for an empty slice: a chest given items, a plant grown, a bed
    /// given a survivor.
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

/// The box the fake checks a player's rights for around a prop at `at`.
pub(crate) fn reach(at: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    (
        at.map(|axis| (axis - REACH).max(0.0)),
        at.map(|axis| axis + REACH),
    )
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
                .ok_or((Status::BAD_ARGS, 0))?;
            if prop.holds != 0 && remove.flags & PropRemove::DISCARD_STATE == 0 {
                return Err((Status::REFUSED, prop.holds));
            }
        }
    }
    for change in &transaction.changes {
        match change {
            FakeChange::PlaceProp(place) => {
                let entity = host
                    .props
                    .iter()
                    .map(|prop| prop.entity + 1)
                    .max()
                    .unwrap_or(FIRST_ENTITY)
                    .max(FIRST_ENTITY);
                host.props.push(FakeProp {
                    entity,
                    item: place.item,
                    at: place.position,
                    holds: 0,
                });
                transaction.created.push(entity);
            }
            FakeChange::RemoveProp(remove) => {
                if let Some(index) = host
                    .props
                    .iter()
                    .position(|prop| prop.entity == remove.entity)
                {
                    transaction.removed.push(host.props.remove(index));
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Takes back `transaction`'s prop changes, as the loader's undo does: a
/// placed prop that now holds state stays, a removed prop that held state
/// does not come back. The reason and the kinds of state that stop it.
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
        if prop.holds != 0 {
            return Err((Status::REFUSED, prop.holds));
        }
    }
    if let Some(held) = transaction.removed.iter().find(|prop| prop.holds != 0) {
        return Err((Status::REFUSED, held.holds));
    }
    host.props
        .retain(|prop| !transaction.created.contains(&prop.entity));
    transaction.created.clear();
    for prop in transaction.removed.drain(..) {
        let entity = host
            .props
            .iter()
            .map(|prop| prop.entity + 1)
            .max()
            .unwrap_or(FIRST_ENTITY)
            .max(FIRST_ENTITY);
        host.props.push(FakeProp { entity, ..prop });
    }
    Ok(())
}
