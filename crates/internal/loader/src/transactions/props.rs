//! A prop a transaction places or removes: the game's placed entity of an item.

use enshwick::abi::TransactionInfo;

use super::cover::{BRICK, Bricks};

const HOLDS_INVENTORY: u32 = TransactionInfo::HOLDS_INVENTORY;
const HOLDS_GROWTH: u32 = TransactionInfo::HOLDS_GROWTH;
const HOLDS_FACTORY_STATION: u32 = TransactionInfo::HOLDS_FACTORY_STATION;
const HOLDS_NPC_BED: u32 = TransactionInfo::HOLDS_NPC_BED;
const HOLDS_STORE_ASSOCIATED_NPC_IDS: u32 = TransactionInfo::HOLDS_STORE_ASSOCIATED_NPC_IDS;
const HOLDS_CUSTOM_PLAYER_STRING: u32 = TransactionInfo::HOLDS_CUSTOM_PLAYER_STRING;

/// What a `FactoryStation` shows: its `FactoryStationState` (0 is
/// `NoRecipe`), and whether a recipe runs or waits next.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FactoryRead {
    pub(crate) state: u8,
    pub(crate) running: bool,
    pub(crate) next: bool,
}

/// What the game holds for a prop that a respawn from its template would
/// lose (server 399, client 426): read from its components, each `None` or
/// `false` when it has none or holds nothing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "each flag is a separate kind of state the game keeps"
)]
pub(crate) struct StateReads {
    /// An item in any slot of its inventory, linked ones included.
    pub(crate) items: bool,
    /// A plant whose `Level` is above its start.
    pub(crate) grown: bool,
    /// Its `FactoryStation`, when it has one.
    pub(crate) factory: Option<FactoryRead>,
    /// A survivor in its `NpcBed`.
    pub(crate) bed_npc: bool,
    /// A survivor in its `StoreAssociatedNpcIds`.
    pub(crate) store_npcs: bool,
    /// Text a player wrote: its `CustomPlayerString`.
    pub(crate) text: bool,
}

impl StateReads {
    /// The kinds of state it holds, as `TransactionInfo::HOLDS_` bits.
    /// Fuel, switches and the Quick Stash toggle are settings, not state.
    pub(crate) fn holds(self) -> u32 {
        let factory = self
            .factory
            .is_some_and(|factory| factory.state != 0 || factory.running || factory.next);
        [
            (self.items, HOLDS_INVENTORY),
            (self.grown, HOLDS_GROWTH),
            (factory, HOLDS_FACTORY_STATION),
            (self.bed_npc, HOLDS_NPC_BED),
            (self.store_npcs, HOLDS_STORE_ASSOCIATED_NPC_IDS),
            (self.text, HOLDS_CUSTOM_PLAYER_STRING),
        ]
        .into_iter()
        .filter(|&(held, _)| held)
        .fold(0, |bits, (_, bit)| bits | bit)
    }
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

/// Metres to the game's 32.32 fixed point.
const FIXED_ONE: f64 = 4_294_967_296.0;
/// Half the side of the box a player's rights are checked for around a
/// prop's position, in metres: the box is enshwick's, since the game keeps
/// no bounds a server can read for a prop.
pub(crate) const REACH: f64 = 2.0;

/// A prop as the world holds it: the template its item places, the item,
/// where it stands and which way it faces.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Prop {
    /// The template GUID it was spawned from.
    pub(crate) template: [u8; 16],
    /// The item it was placed from, its `UsedItem`.
    pub(crate) item: u32,
    /// Where it stands, in the game's 32.32 fixed-point metres.
    pub(crate) position: [i64; 3],
    /// Which way it faces, a quaternion x, y, z, w.
    pub(crate) orientation: [f32; 4],
    /// The item's placement box around the prop, before it turns: the
    /// `EquipmentSetup` `placementAABBmin` and `placementAABBmax`; `None`
    /// when the item has none.
    pub(crate) local_box: Option<([f32; 3], [f32; 3])>,
    /// Whether its item is a logic prop, `IsLogicEntity`, whose placement
    /// the game also announces with a `LogicEntityPlacedEvent`.
    pub(crate) logic: bool,
}

impl Prop {
    /// Its position in metres.
    pub(crate) fn metres(&self) -> [f64; 3] {
        #[expect(
            clippy::cast_precision_loss,
            reason = "a world position in fixed point fits an f64's precision to well below a voxel"
        )]
        self.position.map(|axis| axis as f64 / FIXED_ONE)
    }

    /// Whether `other` is this prop where it was: the same template and
    /// item at the same position.
    pub(crate) fn same_place(&self, other: &Self) -> bool {
        self.template == other.template
            && self.item == other.item
            && self.position == other.position
    }

    /// The box its rights are checked for, from 0: its item's placement
    /// box turned by its orientation, as the game's cursor turns it, else
    /// [`REACH`] metres around its position on every axis.
    pub(crate) fn reach(&self) -> ([f64; 3], [f64; 3]) {
        let at = self.metres();
        let (low, high) = match self.local_box {
            Some((min, max)) => {
                let (min, max) = (min.map(f64::from), max.map(f64::from));
                let mut low = [f64::INFINITY; 3];
                let mut high = [f64::NEG_INFINITY; 3];
                for corner in 0..8 {
                    let local = [0, 1, 2].map(|axis| {
                        if corner & (1 << axis) == 0 {
                            min[axis]
                        } else {
                            max[axis]
                        }
                    });
                    let world = turn(self.orientation, local);
                    for axis in 0..3 {
                        low[axis] = low[axis].min(world[axis]);
                        high[axis] = high[axis].max(world[axis]);
                    }
                }
                (low, high)
            }
            None => ([-REACH; 3], [REACH; 3]),
        };
        (
            [0, 1, 2].map(|axis| (at[axis] + low[axis]).max(0.0)),
            [0, 1, 2].map(|axis| (at[axis] + high[axis]).max(0.0)),
        )
    }

    /// The brick its position lies in.
    pub(crate) fn bricks(&self) -> Bricks {
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a checked position is at 0 or more and within the coordinates a box covers"
        )]
        let voxel = self.metres().map(|axis| (axis * 2.0).max(0.0) as u32);
        Bricks::of_box(voxel.map(|v| v / BRICK * BRICK), [1; 3])
    }

    /// A prop of `item` at `metres`, its template the item's bytes.
    #[cfg(test)]
    #[expect(clippy::cast_possible_truncation, reason = "test positions are small")]
    pub(crate) fn for_tests(item: u32, metres: [f64; 3]) -> Self {
        let mut template = [0; 16];
        template[..4].copy_from_slice(&item.to_le_bytes());
        Self {
            template,
            item,
            position: metres.map(|axis| (axis * FIXED_ONE) as i64),
            orientation: [0.0, 0.0, 0.0, 1.0],
            local_box: None,
            logic: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn items_in_any_linked_slot_are_state() {
        let reads = StateReads {
            items: true,
            ..StateReads::default()
        };
        assert_eq!(reads.holds(), HOLDS_INVENTORY);
    }

    #[test]
    fn a_plant_above_its_start_is_state() {
        let reads = StateReads {
            grown: true,
            ..StateReads::default()
        };
        assert_eq!(reads.holds(), HOLDS_GROWTH);
    }

    #[test]
    fn a_factory_station_with_a_recipe_running_or_queued_is_state() {
        let idle = FactoryRead::default();
        assert_eq!(
            StateReads {
                factory: Some(idle),
                ..StateReads::default()
            }
            .holds(),
            0,
            "an idle station with no recipe holds nothing"
        );
        for busy in [
            FactoryRead { state: 4, ..idle },
            FactoryRead {
                running: true,
                ..idle
            },
            FactoryRead { next: true, ..idle },
        ] {
            let reads = StateReads {
                factory: Some(busy),
                ..StateReads::default()
            };
            assert_eq!(reads.holds(), HOLDS_FACTORY_STATION, "{busy:?}");
        }
    }

    #[test]
    fn survivors_assigned_and_written_text_are_state() {
        let bed = StateReads {
            bed_npc: true,
            ..StateReads::default()
        };
        assert_eq!(bed.holds(), HOLDS_NPC_BED);
        let store = StateReads {
            store_npcs: true,
            ..StateReads::default()
        };
        assert_eq!(store.holds(), HOLDS_STORE_ASSOCIATED_NPC_IDS);
        let sign = StateReads {
            text: true,
            ..StateReads::default()
        };
        assert_eq!(sign.holds(), HOLDS_CUSTOM_PLAYER_STRING);
    }

    #[test]
    fn a_prop_with_nothing_set_holds_no_state_and_kinds_combine() {
        assert_eq!(StateReads::default().holds(), 0);
        let both = StateReads {
            items: true,
            text: true,
            ..StateReads::default()
        };
        assert_eq!(both.holds(), HOLDS_INVENTORY | HOLDS_CUSTOM_PLAYER_STRING);
    }

    #[test]
    fn a_placement_box_turns_with_the_prop() {
        let quarter = core::f32::consts::FRAC_1_SQRT_2;
        let mut prop = Prop::for_tests(1, [20.0; 3]);
        prop.local_box = Some(([-1.0, 0.0, -0.5], [1.0, 2.0, 0.5]));
        assert_eq!(prop.reach(), ([19.0, 20.0, 19.5], [21.0, 22.0, 20.5]));
        prop.orientation = [0.0, quarter, 0.0, quarter];
        let (min, max) = prop.reach();
        let near = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4);
        assert!(near(min, [19.5, 20.0, 19.0]), "{min:?}");
        assert!(near(max, [20.5, 22.0, 21.0]), "{max:?}");
    }

    #[test]
    fn a_prop_s_box_reaches_around_its_position_and_stops_at_zero() {
        let prop = Prop::for_tests(1, [20.0, 1.0, 30.5]);
        assert_eq!(prop.metres(), [20.0, 1.0, 30.5]);
        assert_eq!(prop.reach(), ([18.0, 0.0, 28.5], [22.0, 3.0, 32.5]));
        assert_eq!(prop.bricks().count(), 1);
        assert_eq!(prop.bricks().iter().next().unwrap().0, [5, 0, 7]);
    }
}
