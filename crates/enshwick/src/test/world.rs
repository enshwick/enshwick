//! The world: components, entities, daytime, game settings, weather, terrain and voxels.

use crate::abi::EntityPlace;

use super::FakeHost;
use super::Installed;
use super::table;

impl FakeHost {
    /// The component of type `type_hash` that `entity` has, holding a copy
    /// of `bytes` that the fake keeps, 16-byte aligned, for the rest of the
    /// process; the mod's writes land in that copy.
    ///
    /// # Examples
    ///
    /// ```
    /// let health = [80.0f32.to_le_bytes(), 100.0f32.to_le_bytes()].concat();
    /// let loader = enshwick::test::FakeHost::new()
    ///     .component(4096, 0x027a_c564, &health)
    ///     .template(4096, [0x3a; 16])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn component(mut self, entity: u32, type_hash: u32, bytes: &[u8]) -> Self {
        let mut words = vec![0u128; bytes.len().div_ceil(16).max(1)].into_boxed_slice();
        for (word, chunk) in words.iter_mut().zip(bytes.chunks(16)) {
            let mut le = [0u8; 16];
            le[..chunk.len()].copy_from_slice(chunk);
            *word = u128::from_le_bytes(le);
        }
        let data = Box::leak(words).as_mut_ptr();
        self.components
            .insert((entity, type_hash), (data.expose_provenance(), bytes.len()));
        self
    }

    /// Refuses component writes of type `type_hash`, as the loader refuses
    /// the copies the game keeps for clients.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().refuse_write(0x216e_b5f9).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn refuse_write(mut self, type_hash: u32) -> Self {
        self.refused_writes.insert(type_hash);
        self
    }

    /// Makes the component type `type_hash` the static half of its ECS type,
    /// the template's set-up data: reads find it as
    /// [`FakeHost::component`] gives it, and writes are refused with
    /// [`Status::STATIC_HALF`](crate::abi::Status::STATIC_HALF), as the loader
    /// refuses them.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().static_half(0x139a_fbec).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn static_half(mut self, type_hash: u32) -> Self {
        self.static_halves.insert(type_hash);
        self
    }

    /// An entity in the world with a component of type `type_hash`, at
    /// `position` in metres, for [`World::entities`](crate::world::World::entities)
    /// and [`World::near`](crate::world::World::near).
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .placed(0x027a_c564, 4096, [1.0, 2.0, 3.0])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn placed(mut self, type_hash: u32, entity: u32, position: [f64; 3]) -> Self {
        self.placed.push((
            type_hash,
            EntityPlace {
                entity,
                reserved: 0,
                position: crate::ecs::fixed(position),
            },
        ));
        self
    }

    /// A map marker of the scene, as the loader reads it from the game's
    /// archive for [`Map::markers`](crate::map::Map::markers).
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::MapMarkerInfo;
    ///
    /// let marker = MapMarkerInfo { position: [10.0, 40.0, 20.0], ..MapMarkerInfo::EMPTY };
    /// let loader = enshwick::test::FakeHost::new().map_marker(marker).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn map_marker(mut self, marker: crate::abi::MapMarkerInfo) -> Self {
        self.map_markers.push(marker);
        self
    }

    /// The template GUID of `entity`, in file order.
    ///
    /// # Examples
    ///
    /// See [`FakeHost::component`].
    #[must_use]
    pub fn template(mut self, entity: u32, guid: [u8; 16]) -> Self {
        self.templates.insert(entity, guid);
        self
    }
}

#[expect(
    clippy::unused_self,
    reason = "the fake's state is process-wide; `self` proves it is installed"
)]
impl Installed {
    /// The accounts on the fake server's ban list, in the order banned.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.banned().is_empty());
    /// ```
    #[must_use]
    pub fn banned(&self) -> Vec<u64> {
        table::with(Vec::new(), |s| s.banned.clone())
    }

    /// The accounts whose players the mod removed from the game, by a kick
    /// or a ban, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.removed().is_empty());
    /// ```
    #[must_use]
    pub fn removed(&self) -> Vec<u64> {
        table::with(Vec::new(), |s| s.removed.clone())
    }

    /// The accounts on the fake server's allowlist, in the order listed.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.allowlisted().is_empty());
    /// ```
    #[must_use]
    pub fn allowlisted(&self) -> Vec<u64> {
        table::with(Vec::new(), |s| s.allowlisted.clone())
    }

    /// Stops the world, as between two worlds: the world's reads answer
    /// that none runs.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.stop_world();
    /// ```
    pub fn stop_world(&self) {
        table::with((), |s| s.edits.stopped = true);
    }

    /// Has the scene's map markers still being read while `pending`; as the
    /// loader does, the fake answers the first call of a world that way
    /// whatever this says.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.read_map_markers(true);
    /// ```
    pub fn read_map_markers(&self, pending: bool) {
        table::with((), |s| s.edits.markers_pending = pending);
    }

    /// Sets the clock the fake's `time_of_day` reports; until then the
    /// clock is missing, as before a world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::abi::{DaytimeInfo, Header};
    ///
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_daytime(DaytimeInfo {
    ///     header: Header::of::<DaytimeInfo>(),
    ///     time: 0,
    ///     day_start: 6 * 3_600_000_000_000,
    ///     day_end: 18 * 3_600_000_000_000,
    ///     day_length: 1_800_000_000_000,
    ///     night_length: 720_000_000_000,
    ///     speed: 1.0,
    ///     flags: 0,
    /// });
    /// assert_eq!(loader.daytime().map(|info| info.time), Some(0));
    /// ```
    pub fn set_daytime(&self, info: crate::abi::DaytimeInfo) {
        table::with((), |s| s.daytime = Some(info));
    }

    /// The clock as the mod left it; a time it set moves the clock and its
    /// day flag.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_daytime`].
    #[must_use]
    pub fn daytime(&self) -> Option<crate::abi::DaytimeInfo> {
        table::with(None, |s| s.daytime)
    }

    /// The mod's factor on the clock's speed, once it asked to change it.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.clock_speed(), None);
    /// ```
    #[must_use]
    pub fn clock_speed(&self) -> Option<f32> {
        table::with(None, |s| s.clock_speed)
    }

    /// Sets the game settings the fake reports, the engine's settings struct
    /// as bytes of its reflected size, and their preset; until then no
    /// world runs. A change the mod makes replaces them and makes the
    /// preset custom (4).
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_game_settings(vec![0; 144], 0);
    /// assert_eq!(loader.game_settings().map(|(bytes, _)| bytes.len()), Some(144));
    /// ```
    pub fn set_game_settings(&self, bytes: Vec<u8>, preset: u32) {
        table::with((), |s| s.game_settings = Some((bytes, preset)));
    }

    /// The game settings' bytes and preset as the mod left them.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_game_settings`].
    #[must_use]
    pub fn game_settings(&self) -> Option<(Vec<u8>, u32)> {
        table::with(None, |s| s.game_settings.clone())
    }

    /// The game settings' bytes the mod kept with the world, by offset:
    /// `None` for a byte no kept field covers; empty before any is kept or
    /// after the mod forgot them.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.kept_game_settings().is_empty());
    /// ```
    #[must_use]
    pub fn kept_game_settings(&self) -> Vec<Option<u8>> {
        table::with(Vec::new(), |s| s.kept_settings.clone())
    }

    /// Sets the weather the fake reports: the weights everywhere (clear,
    /// rain, snow, blizzard) and the fronts; until then no world runs.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_weather([1.0, 0.0, 0.0, 0.0], &[]);
    /// assert!(loader.forecasts().is_empty());
    /// ```
    pub fn set_weather(&self, weights: [f32; 4], fronts: &[crate::abi::WeatherFront]) {
        table::with((), |s| s.weather = Some((weights, fronts.to_vec())));
    }

    /// The weather the mod started: effect (`u32::MAX` for the game's
    /// choice) and duration in game nanoseconds.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_weather`].
    #[must_use]
    pub fn forecasts(&self) -> Vec<(u32, i64)> {
        table::with(Vec::new(), |s| s.forecasts.clone())
    }

    /// Sets the terrain materials the fake lists; until then no world runs.
    /// Every edit the mod asks for is recorded and moves one cubic metre.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// loader.set_terrain(&[]);
    /// assert!(loader.terrain_edits().is_empty());
    /// ```
    pub fn set_terrain(&self, materials: &[crate::abi::TerrainMaterialInfo]) {
        table::with((), |s| s.terrain = Some(materials.to_vec()));
    }

    /// The terrain edits the mod asked for, in order.
    ///
    /// # Examples
    ///
    /// See [`Installed::set_terrain`].
    #[must_use]
    pub fn terrain_edits(&self) -> Vec<crate::abi::TerrainEdit> {
        table::with(Vec::new(), |s| s.edits.terrain.clone())
    }

    /// The labels the mod set, as `(entity, text)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.labels().is_empty());
    /// ```
    #[must_use]
    pub fn labels(&self) -> Vec<(u32, String)> {
        table::with(Vec::new(), |s| s.labels.clone())
    }

    /// The entities the mod asked the world to destroy, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.destroyed().is_empty());
    /// ```
    #[must_use]
    pub fn destroyed(&self) -> Vec<u32> {
        table::with(Vec::new(), |s| s.destroyed.clone())
    }

    /// The components the mod asked to write, `(entity, type hash)`, in
    /// order; the fake hands out the data [`FakeHost::component`] gave.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.writes().is_empty());
    /// ```
    #[must_use]
    pub fn writes(&self) -> Vec<(u32, u32)> {
        table::with(Vec::new(), |s| s.writes.clone())
    }

    /// The bytes of the component [`FakeHost::component`] gave `entity`, as
    /// the mod's writes left them.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().component(7, 1, &[4, 2]).install();
    /// assert_eq!(loader.component_bytes(7, 1), Some(vec![4, 2]));
    /// assert_eq!(loader.component_bytes(7, 2), None);
    /// ```
    #[must_use]
    pub fn component_bytes(&self, entity: u32, type_hash: u32) -> Option<Vec<u8>> {
        let (address, len) = table::with(None, |s| {
            s.host.components.get(&(entity, type_hash)).copied()
        })?;
        let data: *const u8 = core::ptr::with_exposed_provenance(address);
        // SAFETY: `component` leaked `len` initialised bytes at `address`,
        // and no write of the mod's runs while the test reads.
        Some(unsafe { core::slice::from_raw_parts(data, len) }.to_vec())
    }

    /// The game events the mod raised, `(type hash, bytes)`, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.emits().is_empty());
    /// ```
    #[must_use]
    pub fn emits(&self) -> Vec<(u32, Vec<u8>)> {
        table::with(Vec::new(), |s| s.emits.clone())
    }

    /// The teleports the mod asked for, in order; an entity the fake does
    /// not know is refused.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.teleports().is_empty());
    /// ```
    #[must_use]
    pub fn teleports(&self) -> Vec<crate::abi::TeleportRequest> {
        table::with(Vec::new(), |s| s.teleports.clone())
    }

    /// The spawns the mod asked for, with the entity ids the fake handed
    /// out, in order.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.spawns().is_empty());
    /// ```
    #[must_use]
    pub fn spawns(&self) -> Vec<(u32, crate::abi::SpawnRequest)> {
        table::with(Vec::new(), |s| s.spawns.clone())
    }

    /// How many saves the mod asked for.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert_eq!(loader.saves(), 0);
    /// ```
    #[must_use]
    pub fn saves(&self) -> usize {
        table::with(0, |s| s.saves)
    }

    /// The voxel boxes the mod read on the game thread, `(min, size)`, in
    /// order, whatever the read answered.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().install();
    /// assert!(loader.voxel_reads().is_empty());
    /// ```
    #[must_use]
    pub fn voxel_reads(&self) -> Vec<([u32; 3], [u32; 3])> {
        table::with(Vec::new(), |s| s.edits.voxel_reads.clone())
    }
}

impl FakeHost {
    /// Loaded voxels of the world: the box of `size` from `min`, its
    /// `voxels` x fastest, then y, then z. A read answers from the boxes,
    /// the last given first where they overlap, and finds a box it does not
    /// cover wholly not loaded, as the loader does where the world is not
    /// loaded yet;
    /// once the fake has boxes, a terrain edit whose position lies in none
    /// is not loaded either.
    /// Past the world's edge ([`FakeHost::world_extent`]) a read is outside
    /// the world.
    ///
    /// # Panics
    ///
    /// When `voxels` does not hold one voxel for each place in the box.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::voxels::{Voxel, VoxelMaterial, VoxelPos};
    ///
    /// let rock = Voxel::new(VoxelMaterial::Terrain(9), 255);
    /// let loader = enshwick::test::FakeHost::new()
    ///     .voxels(VoxelPos::new(0, 0, 0), [2, 1, 1], &[rock, Voxel::default()])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn voxels(
        mut self,
        min: crate::voxels::VoxelPos,
        size: [u32; 3],
        voxels: &[crate::voxels::Voxel],
    ) -> Self {
        let count = size.iter().map(|&axis| axis as usize).product::<usize>();
        assert_eq!(
            voxels.len(),
            count,
            "a fake voxel box of {size:?} takes {count} voxels"
        );
        self.voxels.push(VoxelGrid {
            min: min.to_array(),
            size,
            voxels: voxels.iter().map(|voxel| voxel.to_abi()).collect(),
        });
        self
    }
}

impl FakeHost {
    /// The world's end on each axis, in voxels: a read of a box past it,
    /// and an edit or a transaction's change whose position lies past it,
    /// is outside the world, as the loader refuses them. The loader takes
    /// the edge across from the game and its own height; the fake checks
    /// an edit's position, where the loader checks every voxel the edit can
    /// write. Without it, the fake has no edge.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new().world_extent([20_480, 2048, 20_480]).install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub const fn world_extent(mut self, end: [u32; 3]) -> Self {
        self.extent = Some(end);
        self
    }
}

/// Whether the box of `size` from `min` leaves a world ending at `end`.
pub(crate) fn outside(end: Option<[u32; 3]>, min: [u32; 3], size: [u32; 3]) -> bool {
    end.is_some_and(|end| {
        (0..3).any(|axis| {
            min[axis]
                .checked_add(size[axis])
                .is_none_or(|stop| stop > end[axis])
        })
    })
}

/// Whether the edit's position lies past a world ending at `end`.
pub(crate) fn edit_outside(end: Option<[u32; 3]>, edit: &crate::abi::TerrainEdit) -> bool {
    end.is_some_and(|end| {
        edit.position
            .iter()
            .zip(end)
            .any(|(&metres, stop)| f64::from(metres) * 2.0 >= f64::from(stop))
    })
}

/// Whether the edit's position lies in one of `grids`, when the fake has
/// any: the loader checks every column of bricks the edit can write.
pub(crate) fn edit_loaded(grids: &[VoxelGrid], edit: &crate::abi::TerrainEdit) -> bool {
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a checked edit's position is finite and at 0 or more; `as` saturates"
    )]
    let at = edit.position.map(|metres| (f64::from(metres) * 2.0) as u32);
    grids.is_empty() || grids.iter().any(|grid| grid.get(at).is_some())
}

/// A box of loaded voxels the fake answers reads from.
#[derive(Clone, Debug)]
pub(crate) struct VoxelGrid {
    min: [u32; 3],
    size: [u32; 3],
    voxels: Vec<crate::abi::Voxel>,
}

impl VoxelGrid {
    /// The voxel at `at`, when the grid holds it.
    fn get(&self, at: [u32; 3]) -> Option<crate::abi::Voxel> {
        let mut index = 0usize;
        for axis in (0..3).rev() {
            let offset = at[axis].checked_sub(self.min[axis])?;
            if offset >= self.size[axis] {
                return None;
            }
            index = index * self.size[axis] as usize + offset as usize;
        }
        self.voxels.get(index).copied()
    }

    /// The voxels of the box of `size` from `min`, x fastest, then y, then
    /// z, from the last of `grids` that holds each; `None` when one is in
    /// none of them.
    pub(crate) fn read(
        grids: &[Self],
        min: [u32; 3],
        size: [u32; 3],
    ) -> Option<Vec<crate::abi::Voxel>> {
        let [width, height, depth] = size;
        let mut voxels = Vec::new();
        for z in 0..depth {
            for y in 0..height {
                for x in 0..width {
                    let at = [min[0] + x, min[1] + y, min[2] + z];
                    voxels.push(grids.iter().rev().find_map(|grid| grid.get(at))?);
                }
            }
        }
        Some(voxels)
    }
}

impl FakeHost {
    /// A Flame Altar's building zone from `min` to `max` in metres, where
    /// [`World::can_build`](crate::world::World::can_build) asks for
    /// `CanEditBase`.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .building_zone([0.0, 0.0, 0.0], [40.0, 40.0, 40.0])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn building_zone(mut self, min: [f64; 3], max: [f64; 3]) -> Self {
        self.zones.building.push(FakeBox { min, max });
        self
    }

    /// A box of the world's building area; once one is given, a corner
    /// outside every such box is refused, as past the world's edge.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .building_area([0.0, 0.0, 0.0], [4096.0, 1024.0, 4096.0])
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn building_area(mut self, min: [f64; 3], max: [f64; 3]) -> Self {
        self.zones.area.push(FakeBox { min, max });
        self
    }

    /// A no-build zone from `min` to `max` in metres; only a `strict` one
    /// refuses an edit, as the game's own rule for an edit without an item.
    ///
    /// # Examples
    ///
    /// ```
    /// let loader = enshwick::test::FakeHost::new()
    ///     .no_build_zone([10.0, 0.0, 10.0], [20.0, 10.0, 20.0], true)
    ///     .install();
    /// # let _ = loader;
    /// ```
    #[must_use]
    pub fn no_build_zone(mut self, min: [f64; 3], max: [f64; 3], strict: bool) -> Self {
        self.zones.no_build.push((FakeBox { min, max }, strict));
        self
    }
}

/// A box in metres.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FakeBox {
    min: [f64; 3],
    max: [f64; 3],
}

impl FakeBox {
    /// Whether the boxes overlap with some thickness on every axis: boxes
    /// that share only a face do not.
    fn overlaps(&self, min: [f64; 3], max: [f64; 3]) -> bool {
        (0..3).all(|a| self.min[a] < max[a] && min[a] < self.max[a])
    }

    /// Whether `point` lies inside, the high faces excluded.
    fn holds(&self, point: [f64; 3]) -> bool {
        (0..3).all(|a| self.min[a] <= point[a] && point[a] < self.max[a])
    }
}

/// The zones the fake answers build rights from.
#[derive(Clone, Debug, Default)]
pub(crate) struct FakeZones {
    building: Vec<FakeBox>,
    area: Vec<FakeBox>,
    no_build: Vec<(FakeBox, bool)>,
}

impl FakeZones {
    /// Whether a save keeps changes in the box from `min` to `max`, as the
    /// loader decides: wholly inside one building zone, half a metre in
    /// from its edges.
    pub(crate) fn kept(&self, min: [f64; 3], max: [f64; 3]) -> bool {
        const MARGIN: f64 = 0.5;
        self.building.iter().any(|zone| {
            (0..3).all(|a| min[a] >= zone.min[a] + MARGIN && max[a] <= zone.max[a] - MARGIN)
        })
    }

    /// The refusal of an edit of the box from `min` to `max` by a player
    /// with the permission mask `permissions`, as the loader decides: the
    /// game's terrain tool rule for the permissions, then the building area
    /// and strict no-build tests Enshwick adds from the building cursor.
    pub(crate) fn refusal(&self, permissions: u8, min: [f64; 3], max: [f64; 3]) -> u32 {
        use crate::abi::{BuildRights, PlayerInfo};
        if permissions & PlayerInfo::CAN_EDIT_WORLD == 0 {
            return BuildRights::NO_CAN_EDIT_WORLD;
        }
        if permissions & PlayerInfo::CAN_EDIT_BASE == 0
            && self.building.iter().any(|zone| zone.overlaps(min, max))
        {
            return BuildRights::NO_CAN_EDIT_BASE;
        }
        let in_area = |corner| self.area.is_empty() || self.area.iter().any(|b| b.holds(corner));
        if !in_area(min) || !in_area(max) {
            return BuildRights::OUTSIDE_BUILDING_ZONE;
        }
        if self
            .no_build
            .iter()
            .any(|(zone, strict)| *strict && zone.overlaps(min, max))
        {
            return BuildRights::INSIDE_STRICT_BUILD_INHIBITOR_ZONE;
        }
        BuildRights::ALLOWED
    }
}
