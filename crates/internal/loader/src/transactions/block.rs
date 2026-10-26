//! A building block a transaction places: checked at commit, written by the game at its step.

use enshwick::abi::Status;

use super::paste::Region;

/// The game's box for a block write, built by its own block builder from
/// the shape, the position and the rotation (server 399, client 426); the
/// loader keeps its bytes and hands them back to the write unchanged.
#[repr(C, align(8))]
#[derive(Clone, Copy, Debug)]
pub(crate) struct GameBox(pub(crate) [u8; GameBox::SIZE]);

impl GameBox {
    /// The bytes the game's builder writes.
    pub(crate) const SIZE: usize = 0x50;
    /// Where the box keeps its lowest voxel, `i32` on each axis.
    pub(crate) const MIN: usize = 0x20;
    /// Where the box keeps its voxels along each axis once turned, `u32`
    /// each.
    pub(crate) const ROTATED_SIZE: usize = 0x2c;

    fn word(&self, at: usize) -> [u8; 4] {
        let mut word = [0; 4];
        word.copy_from_slice(&self.0[at..at + 4]);
        word
    }

    /// The voxels the block writes; [`Status::BAD_ARGS`] for a box below
    /// the world's corner, empty or past the coordinates a box covers.
    pub(crate) fn region(&self) -> Result<Region, Status> {
        let mut min = [0; 3];
        let mut size = [0; 3];
        for axis in 0..3 {
            let low = i32::from_le_bytes(self.word(Self::MIN + 4 * axis));
            min[axis] = u32::try_from(low).map_err(|_| Status::BAD_ARGS)?;
            size[axis] = u32::from_le_bytes(self.word(Self::ROTATED_SIZE + 4 * axis));
        }
        Region::new(min, size)
    }
}

/// The materials a byte names.
pub(crate) const MATERIALS: usize = 0x100;
/// The first building voxel material; building configs number theirs from
/// it, 0-based.
const BUILDING_FIRST: usize = 0x80;
/// The game's `Unbreakable` hardness: a material this hard or harder is
/// never written over.
const UNBREAKABLE: u8 = 5;

/// The voxel materials a building block may write over, one byte each, as
/// the game builds the table for an ordinary building shape (server 399,
/// client 426): air; a terrain material whose config is softer than
/// `Unbreakable`; a building material whose config is (its material
/// stored 0-based from 0x80); nothing else. `terrain` and `building` hold
/// each config's material and hardness.
pub(crate) fn replace_table(terrain: &[(u8, u8)], building: &[(u8, u8)]) -> [u8; MATERIALS] {
    let mut table = [0u8; MATERIALS];
    table[0] = 1;
    for &(material, hardness) in terrain {
        if (1..BUILDING_FIRST).contains(&usize::from(material)) && hardness < UNBREAKABLE {
            table[usize::from(material)] = 1;
        }
    }
    for &(material, hardness) in building {
        if let Some(slot) = table.get_mut(BUILDING_FIRST + usize::from(material))
            && hardness < UNBREAKABLE
        {
            *slot = 1;
        }
    }
    table
}

/// Where the game's place and tear-down events keep their fields, from
/// reflection: the event's size and each field's offset; `item` only in
/// the place event.
#[derive(Clone, Copy, Debug)]
pub(crate) struct EventLayout {
    pub(crate) size: usize,
    pub(crate) position: usize,
    pub(crate) orientation: usize,
    pub(crate) min: usize,
    pub(crate) max: usize,
    pub(crate) material: usize,
    pub(crate) item: Option<usize>,
    pub(crate) owner: usize,
}

/// An event's bytes being filled; `None` once a field passes its end.
struct Event(Option<Vec<u8>>);

impl Event {
    fn put(&mut self, at: usize, bytes: &[u8]) {
        let fits = self.0.as_ref().is_some_and(|event| {
            at.checked_add(bytes.len())
                .is_some_and(|end| end <= event.len())
        });
        match &mut self.0 {
            Some(event) if fits => event[at..at + bytes.len()].copy_from_slice(bytes),
            _ => self.0 = None,
        }
    }

    fn floats<const N: usize>(&mut self, at: usize, values: [f32; N]) {
        for (i, value) in values.iter().enumerate() {
            self.put(at + 4 * i, &value.to_le_bytes());
        }
    }
}

/// A point in metres as the game's events keep it, `f32` on each axis.
#[expect(
    clippy::cast_possible_truncation,
    reason = "world positions in metres fit an f32 as the game's events keep them"
)]
fn metres(point: [f64; 3]) -> [f32; 3] {
    point.map(|axis| axis as f32)
}

impl EventLayout {
    /// The event's bytes with the box of `region`, the owner `actor`, and
    /// the place `position` turned by `rotation`; the time stamp is left
    /// for the game's push.
    fn event(self, region: Region, position: [f64; 3], rotation: [f32; 4], actor: u32) -> Event {
        let (min, max) = region.metres();
        let mut event = Event(Some(vec![0; self.size]));
        event.floats(self.position, metres(position));
        event.floats(self.orientation, rotation);
        event.floats(self.min, metres(min));
        event.floats(self.max, metres(max));
        event.put(self.material, &0u32.to_le_bytes());
        event.put(self.owner, &actor.to_le_bytes());
        event
    }

    /// The `BuildingPlaceEvent` of `block` placed for `actor`, 0 for none,
    /// tracking its shape item with its material's feedback, as the game's
    /// place does (server 399, client 426); `None` when a field passes the
    /// event's size.
    pub(crate) fn place(self, block: &Block, actor: u32) -> Option<Vec<u8>> {
        let mut event = self.event(block.region, block.position, block.rotation, actor);
        event.put(self.material, &block.feedback.to_le_bytes());
        event.put(self.item?, &block.shape.to_le_bytes());
        event.0
    }

    /// The `BuildingTearDownEvent` of the building voxels of `region` torn
    /// down for `actor`, 0 for none.
    pub(crate) fn tear_down(self, region: Region, actor: u32) -> Option<Vec<u8>> {
        let (min, max) = region.metres();
        let middle = [0, 1, 2].map(|axis| f64::midpoint(min[axis], max[axis]));
        self.event(region, middle, [0.0, 0.0, 0.0, 1.0], actor).0
    }
}

/// A block of a building shape and material, placed at a position with a
/// rotation, as the game places it.
#[derive(Clone, Debug)]
pub(crate) struct Block {
    /// The shape's item, which the place event tracks.
    pub(crate) shape: u32,
    /// The feedback the game gives the material, for the place event.
    pub(crate) feedback: u32,
    /// The material's voxel id, from the item's `placeVoxelMaterialId`.
    pub(crate) material: u8,
    /// Where the block's middle lies, in metres.
    pub(crate) position: [f64; 3],
    /// Its rotation, a quaternion x, y, z, w.
    pub(crate) rotation: [f32; 4],
    /// The voxels it writes, from the game's box at commit; the step
    /// builds the box again and writes only if it gives the same.
    pub(crate) region: Region,
}

impl Block {
    /// A block of `material` over `region`, with no shape behind it.
    #[cfg(test)]
    pub(crate) fn for_tests(region: Region, material: u8) -> Self {
        let (min, max) = region.metres();
        Self {
            shape: 0,
            feedback: 0,
            material,
            position: [0, 1, 2].map(|axis| f64::midpoint(min[axis], max[axis])),
            rotation: [0.0, 0.0, 0.0, 1.0],
            region,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_box(min: [i32; 3], size: [u32; 3]) -> GameBox {
        let mut bytes = [0u8; GameBox::SIZE];
        for axis in 0..3 {
            let at = GameBox::MIN + 4 * axis;
            bytes[at..at + 4].copy_from_slice(&min[axis].to_le_bytes());
            let at = GameBox::ROTATED_SIZE + 4 * axis;
            bytes[at..at + 4].copy_from_slice(&size[axis].to_le_bytes());
        }
        GameBox(bytes)
    }

    #[test]
    fn the_game_s_box_gives_the_block_s_voxels() {
        let region = game_box([196, 76, 196], [8, 8, 8]).region().unwrap();
        assert_eq!(region.bounds(), ([196, 76, 196], [8, 8, 8]));
    }

    #[test]
    fn a_game_box_below_the_world_or_empty_is_refused() {
        assert!(game_box([-1, 0, 0], [8, 8, 8]).region().is_err());
        assert!(game_box([0, 0, 0], [0, 8, 8]).region().is_err());
    }

    fn layout() -> EventLayout {
        EventLayout {
            size: 0x48,
            position: 0x8,
            orientation: 0x14,
            min: 0x24,
            max: 0x30,
            material: 0x3c,
            item: Some(0x40),
            owner: 0x44,
        }
    }

    fn f32_at(bytes: &[u8], at: usize) -> f32 {
        f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    fn u32_at(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
    }

    #[test]
    fn a_place_event_carries_the_block_its_box_item_and_owner() {
        let mut block = Block::for_tests(Region::new([196, 76, 196], [8, 8, 8]).unwrap(), 0x83);
        block.shape = 66;
        block.feedback = 5;
        let bytes = layout().place(&block, 7).unwrap();
        assert_eq!(bytes.len(), 0x48);
        assert_eq!(f32_at(&bytes, 0x8), 100.0);
        assert_eq!(f32_at(&bytes, 0x14 + 12), 1.0, "w of the rotation");
        assert_eq!(f32_at(&bytes, 0x24), 98.0);
        assert_eq!(f32_at(&bytes, 0x30 + 4), 42.0);
        assert_eq!(u32_at(&bytes, 0x3c), 5, "the material's feedback");
        assert_eq!(
            (u32_at(&bytes, 0x40), u32_at(&bytes, 0x44)),
            (66, 7),
            "the shape tracks the block"
        );
    }

    #[test]
    fn a_tear_down_event_carries_the_box_and_its_owner() {
        let layout = EventLayout {
            item: None,
            owner: 0x40,
            ..layout()
        };
        let region = Region::new([196, 76, 196], [8, 8, 8]).unwrap();
        let bytes = layout.tear_down(region, 0).unwrap();
        assert_eq!(f32_at(&bytes, 0x8), 100.0);
        assert_eq!(f32_at(&bytes, 0x24 + 4), 38.0);
        assert_eq!(u32_at(&bytes, 0x40), 0);
    }

    #[test]
    fn a_block_replaces_air_and_breakable_configured_materials_only() {
        let table = replace_table(&[(2, 1), (9, 5)], &[(3, 0), (4, 5)]);
        assert_eq!(table[0], 1, "air");
        assert_eq!(table[2], 1, "a terrain material below Unbreakable");
        assert_eq!(table[9], 0, "an Unbreakable terrain material");
        assert_eq!(table[5], 0, "a terrain material without a config");
        assert_eq!(table[0x83], 1, "building material 3, stored 0-based");
        assert_eq!(table[0x84], 0, "an Unbreakable building material");
        assert_eq!(table[0x90], 0, "a building material without a config");
        assert_eq!(table.iter().map(|&may| usize::from(may)).sum::<usize>(), 3);
    }

    #[test]
    fn a_layout_whose_fields_pass_its_size_builds_nothing() {
        let narrow = EventLayout {
            size: 0x40,
            ..layout()
        };
        let block = Block::for_tests(Region::new([0; 3], [8; 3]).unwrap(), 0x83);
        assert!(narrow.place(&block, 0).is_none());
    }
}
