//! What the fake's world answers, as the test sets it.

use enshwick_core::abi::EntityPlace;

use super::{FakeZones, VoxelGrid, props, respawn};
use crate::test::FakeHost;

impl FakeHost {
    /// Plays a game build without the world's entity commands: destroy,
    /// `set_saved`, teleport and component writes answer
    /// [`Status::UNAVAILABLE`](enshwick_core::abi::Status::UNAVAILABLE), as they do
    /// on a client that hosts no game.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().world().without_entity_commands().install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn without_entity_commands(mut self) -> Self {
        let _new = self.world.lacks.insert(Lacks::EntityCommands);
        self
    }

    /// Plays a game build whose table lacks the world's reads: component
    /// reads, and the inventory, buff and entity reads made of them, answer
    /// [`Status::UNAVAILABLE`](enshwick_core::abi::Status::UNAVAILABLE) on
    /// the game thread too, as the loader does on such a build.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().world().without_world_reads().install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn without_world_reads(mut self) -> Self {
        let _new = self.world.lacks.insert(Lacks::WorldReads);
        self
    }

    /// Plays a game build whose table lacks the save code: a save and a
    /// save hold, or letting one go, answer
    /// [`Status::UNAVAILABLE`](enshwick_core::abi::Status::UNAVAILABLE), as
    /// the loader does on such a build, so a mod tests its reply when the
    /// game cannot save.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().world().without_saves().install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn without_saves(mut self) -> Self {
        let _new = self.world.lacks.insert(Lacks::SaveCode);
        self
    }
}

impl WorldPresets {
    /// Whether the build lacks `part`.
    pub(crate) fn lacks(&self, part: Lacks) -> bool {
        self.lacks.contains(&part)
    }

    /// Whether the build lacks the world's reads, for the base domain's
    /// component reads, which may not name this domain's types.
    pub(crate) fn lacks_world_reads(&self) -> bool {
        self.lacks(Lacks::WorldReads)
    }
}

/// Whether the fake's game build has the base code the loader hooks to
/// raise the base limits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum BaseCode {
    /// The platform knows it.
    #[default]
    Known,
    /// The platform raises the zones but cannot hold bases past 16.
    ZonesOnly,
    /// An unknown build: the game keeps its own limits.
    Unknown,
}

/// The box the game's world scene keeps players in, lowest corner first, in
/// metres, as the loader read it on server 26430253.
pub(crate) const GAME_WORLD_BOX: ([f64; 3], [f64; 3]) = ([0.0; 3], [10_240.0, 2_560.0, 10_240.0]);

/// A piece of the world's code a fake game build lacks, so the calls made
/// of it answer `UNAVAILABLE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Lacks {
    /// Destroy, `set_saved`, teleport and component writes.
    EntityCommands,
    /// Component reads, on the game thread too.
    WorldReads,
    /// Saves and save holds.
    SaveCode,
    /// The code altars are read from.
    AltarCode,
}

/// What the fake's world answers, as the test set it.
#[derive(Clone, Debug, Default)]
pub(crate) struct WorldPresets {
    pub(crate) lacks: rustc_hash::FxHashSet<Lacks>,
    pub(crate) base_code: BaseCode,
    /// The game's level cap and need per level, once the test set them.
    pub(crate) level_table: Option<super::progress::LevelTable>,
    pub(crate) placed: Vec<(u32, EntityPlace)>,
    pub(crate) map_markers: Vec<enshwick_core::abi::MapMarkerInfo>,
    pub(crate) map_frame: Option<enshwick_core::abi::MapFrameInfo>,
    pub(crate) refused_writes: rustc_hash::FxHashSet<u32>,
    pub(crate) static_halves: rustc_hash::FxHashSet<u32>,
    pub(crate) voxels: Vec<VoxelGrid>,
    pub(crate) zones: FakeZones,
    pub(crate) extent: Option<[u32; 3]>,
    /// The box the scene keeps players and entities in, lowest corner
    /// first, in metres; a teleport past it is outside the world. `None` is
    /// the game's own, [`GAME_WORLD_BOX`].
    pub(crate) world_box: Option<([f64; 3], [f64; 3])>,
    pub(crate) blueprints: rustc_hash::FxHashMap<u32, [u32; 3]>,
    pub(crate) props: Vec<props::FakeProp>,
    /// The items whose props are logic triggers.
    pub(crate) trigger_items: Vec<u32>,
    /// The items whose props are plants.
    pub(crate) growing_items: Vec<u32>,
    /// The items that place something that is no prop: a creature, a
    /// flame altar.
    pub(crate) no_prop_items: Vec<u32>,
    /// Each item's placement box, lowest and highest corner before it
    /// turns.
    pub(crate) placement_boxes: Vec<(u32, [f32; 3], [f32; 3])>,
    /// The spawn points in the world.
    pub(crate) spawn_points: Vec<respawn::FakeSpawnPoint>,
    /// Each character's respawn point, by the character's entity.
    pub(crate) respawn_points: rustc_hash::FxHashMap<u32, enshwick_core::Entity>,
    pub(crate) building_materials: rustc_hash::FxHashMap<u32, u8>,
}
