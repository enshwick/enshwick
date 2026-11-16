//! Terrain materials of mods' own, and the ground of removed mods' kept as
//! the fallback each declared.
//!
//! A voxel keeps its material as one byte. Each palette world names the
//! bytes by GUID, and a save maps its voxels to bytes by those GUIDs as it
//! loads, so a byte holds for one run only; a voxel whose GUID no palette
//! names loads as air (dedicated server 399). The game leaves terrain ids 117
//! to 127 unnamed (data 1287369). A mod's material takes the first one free
//! in load order: the loader names it in every palette world, adds its
//! terraforming rules (a copy of the vanilla material it is like, with the
//! mod's changes) and copies that material's effects entry and, on the
//! client, its look.
//!
//! The clone record keeps every material ever added with its fallback.
//! While no mod adds a recorded one, the loader names its GUID in the next
//! free id as a copy of the fallback, so its ground loads as the fallback,
//! and is the mod's material again once the mod is back.

use enshwick_base::resources::Value;
use enshwick_core::abi::TerrainMaterialAdd;
use enshwick_core::content::Guid;

use crate::engine::registry::Sealed;

/// Terrain ids, below the building ids: one look entry each.
const TERRAIN_IDS: usize = 128;
/// The bytes of one look entry: its first layer and its layer count, two
/// `u16` each (client data 1287369).
const LOOK_ENTRY: usize = 4;

/// The game's names of the fields the loader reads and writes.
const MATERIAL_GUIDS: &str = "materialGuids";
const TERRAIN_CONFIGS: &str = "terrainConfigs";
const TERRAIN_MATERIAL: &str = "terrainMaterial";
const MATERIAL_ID_VALUE: &str = "value";
const TERRAIN_ITEM: &str = "terrainItem";
const HARDNESS: &str = "hardness";
const HEALTH_POINTS: &str = "healthPoints";
const CAN_BE_FLATTENED: &str = "canBeFlattened";
const MATERIAL_TYPE: &str = "materialType";
const BAKING_VOXEL_MATERIALS: &str = "bakingVoxelMaterialDataBlock";
const DATA: &str = "data";

/// Why a terrain material was not added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum TerrainMaterialError {
    /// Asked after init.
    #[error("asked after init")]
    TooLate,
    /// The archive lacks the palette or the terraforming rules.
    #[error("this build's archive lacks the terrain palette or rules")]
    Unread,
    /// A material it copies or falls back to that the game lacks.
    #[error("terrain id {0} is none of the game's terrain materials")]
    NotVanilla(u8),
    /// A change the loader does not know, or a health below 1.
    #[error("a change it does not know, or a health below 1")]
    BadChanges,
    /// Its GUID is a terrain material already.
    #[error("its id is a terrain material already")]
    Taken,
    /// Every free terrain id is taken by a mod loaded before.
    #[error("the game's {0} free terrain ids are taken by mods loaded before it")]
    Full(usize),
}

impl From<Sealed> for TerrainMaterialError {
    fn from(Sealed: Sealed) -> Self {
        Self::TooLate
    }
}

/// The rules a material sets; each `None` keeps the one it copies.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Changes {
    item: Option<Guid>,
    hardness: Option<u8>,
    health: Option<i32>,
    flattens: Option<bool>,
}

impl Changes {
    /// The changes `request` names; `None` for an unknown one or a health
    /// below 1.
    fn of(request: &TerrainMaterialAdd) -> Option<Self> {
        let set = |bit| request.changes & bit != 0;
        if !enshwick_core::__private::rules::terrain_material_changes_valid(request) {
            return None;
        }
        Some(Self {
            item: set(TerrainMaterialAdd::ITEM).then_some(request.item),
            hardness: set(TerrainMaterialAdd::HARDNESS).then_some(request.hardness),
            health: set(TerrainMaterialAdd::HEALTH).then_some(request.health),
            flattens: set(TerrainMaterialAdd::FLATTENS)
                .then_some(request.flags & enshwick_core::abi::TerrainMaterialInfo::FLATTENS != 0),
        })
    }

    /// Writes the changes into the terraforming rule `config`.
    fn apply(&self, config: &mut Value) {
        let mut set = |name: &str, value: Value| {
            if let Some(field) = config.field_mut(name) {
                *field = value;
            }
        };
        if let Some(item) = self.item {
            set(TERRAIN_ITEM, Value::Guid(item));
        }
        if let Some(hardness) = self.hardness {
            set(HARDNESS, Value::Uint(u64::from(hardness)));
        }
        if let Some(health) = self.health {
            set(HEALTH_POINTS, Value::Int(i64::from(health)));
        }
        if let Some(flattens) = self.flattens {
            set(CAN_BE_FLATTENED, Value::Bool(flattens));
        }
    }
}

/// A terrain material the loader serves: a mod's, or a removed mod's kept
/// as its fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Material {
    /// The GUID the palette names it by.
    guid: Guid,
    /// Its terrain id for this run.
    id: u8,
    /// The game's material whose rules, effects and look it copies.
    like: u8,
    /// The game's material its ground loads as without it.
    fallback: u8,
    changes: Changes,
}

/// The game's own terrain materials, from its archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Palette {
    /// The GUID each terrain id names in the palette worlds.
    names: [Option<Guid>; TERRAIN_IDS],
    /// Whether each terrain id has a terraforming rule.
    ruled: [bool; TERRAIN_IDS],
}

impl Palette {
    /// The palette of the palette worlds `worlds` and the rules `rules`:
    /// a terrain id any world names takes the first world's name for it.
    fn read<'a>(worlds: impl IntoIterator<Item = &'a Value>, rules: &Value) -> Option<Self> {
        let mut names = [None; TERRAIN_IDS];
        for world in worlds {
            let Some(Value::List(guids)) = world.field(MATERIAL_GUIDS) else {
                return None;
            };
            for (name, guid) in names.iter_mut().zip(guids) {
                if let Value::Guid(guid) = guid
                    && *guid != Guid::NIL
                {
                    name.get_or_insert(*guid);
                }
            }
        }
        let Some(Value::List(configs)) = rules.field(TERRAIN_CONFIGS) else {
            return None;
        };
        let mut ruled = [false; TERRAIN_IDS];
        for config in configs {
            if let Some(id) = config_id(config)
                && let Some(slot) = ruled.get_mut(usize::from(id))
            {
                *slot = true;
            }
        }
        Some(Self { names, ruled })
    }

    /// Whether `id` is one of the game's terrain materials: named and
    /// ruled.
    fn holds(&self, id: u8) -> bool {
        let at = usize::from(id);
        id != 0 && self.names.get(at).is_some_and(Option::is_some) && self.ruled[at]
    }

    /// The terrain ids the game leaves free, in order: named by no palette
    /// world and ruled by no terraforming rule; air, id 0, never.
    fn free(&self) -> Vec<u8> {
        (1..=u8::MAX)
            .take(TERRAIN_IDS - 1)
            .filter(|&id| self.names[usize::from(id)].is_none() && !self.ruled[usize::from(id)])
            .collect()
    }

    /// The terrain id the palette names `guid` by.
    fn id_of(&self, guid: Guid) -> Option<u8> {
        let at = self.names.iter().position(|name| *name == Some(guid))?;
        u8::try_from(at).ok()
    }

    /// The GUID the palette names terrain id `id` by.
    fn guid_of(&self, id: u8) -> Option<Guid> {
        self.names.get(usize::from(id)).copied().flatten()
    }
}

/// The terrain id a terraforming rule is for.
fn config_id(config: &Value) -> Option<u8> {
    match config.field(TERRAIN_MATERIAL)?.field(MATERIAL_ID_VALUE)? {
        Value::Uint(id) => u8::try_from(*id).ok(),
        _ => None,
    }
}

/// The material `request` asks for, placed after `added` in `free`: its
/// terrain id.
fn place(
    palette: &Palette,
    added: &[Material],
    request: &TerrainMaterialAdd,
) -> Result<Material, TerrainMaterialError> {
    for id in [request.like, request.fallback] {
        if !palette.holds(id) {
            return Err(TerrainMaterialError::NotVanilla(id));
        }
    }
    let changes = Changes::of(request).ok_or(TerrainMaterialError::BadChanges)?;
    if request.guid == Guid::NIL || added.iter().any(|m| m.guid == request.guid) {
        return Err(TerrainMaterialError::Taken);
    }
    let free = palette.free();
    let id = *free
        .get(added.len())
        .ok_or(TerrainMaterialError::Full(free.len()))?;
    Ok(Material {
        guid: request.guid,
        id,
        like: request.like,
        fallback: request.fallback,
        changes,
    })
}

/// The recorded materials `recorded`, each a GUID and its fallback's GUID,
/// that no material of `added` is: each as a copy of its fallback in the
/// next free id after `added`. Then the GUIDs left out: no free id was left
/// for them, or the game no longer has their fallback.
fn orphans(
    palette: &Palette,
    added: &[Material],
    recorded: &[(Guid, Guid)],
) -> (Vec<Material>, Vec<Guid>) {
    let mut free = palette.free().into_iter().skip(added.len());
    let (mut kept, mut lost) = (Vec::new(), Vec::new());
    for &(guid, fallback) in recorded {
        if added.iter().any(|m| m.guid == guid) || palette.id_of(guid).is_some() {
            continue;
        }
        match (
            palette.id_of(fallback).filter(|&id| palette.holds(id)),
            free.next(),
        ) {
            (Some(fallback), Some(id)) => kept.push(Material {
                guid,
                id,
                like: fallback,
                fallback,
                changes: Changes::default(),
            }),
            _ => lost.push(guid),
        }
    }
    (kept, lost)
}

/// Names `materials` in the palette world `world`.
fn name_in(world: &mut Value, materials: &[Material]) -> Option<()> {
    let Value::List(guids) = world.field_mut(MATERIAL_GUIDS)? else {
        return None;
    };
    for material in materials {
        *guids.get_mut(usize::from(material.id))? = Value::Guid(material.guid);
    }
    Some(())
}

/// Adds the terraforming rule of each of `materials` to `rules`: a copy of
/// its `like`'s with its changes.
fn add_rules(rules: &mut Value, materials: &[Material]) -> Option<()> {
    let Value::List(configs) = rules.field_mut(TERRAIN_CONFIGS)? else {
        return None;
    };
    for material in materials {
        let mut config = configs
            .iter()
            .find(|config| config_id(config) == Some(material.like))?
            .clone();
        *config
            .field_mut(TERRAIN_MATERIAL)?
            .field_mut(MATERIAL_ID_VALUE)? = Value::Uint(u64::from(material.id));
        material.changes.apply(&mut config);
        configs.push(config);
    }
    Some(())
}

/// Gives each of `materials` its `like`'s entry of the effects table
/// `effects`, one entry per voxel id.
fn copy_effects(effects: &mut Value, materials: &[Material]) -> Option<()> {
    let Value::List(types) = effects.field_mut(MATERIAL_TYPE)? else {
        return None;
    };
    for material in materials {
        let like = types.get(usize::from(material.like))?.clone();
        *types.get_mut(usize::from(material.id))? = like;
    }
    Some(())
}

/// Gives each of `materials` its `like`'s look in the client's material
/// blending `blending`: its entry of the baking block, which names the
/// layers a terrain id draws with.
fn copy_looks(blending: &mut Value, materials: &[Material]) -> Option<()> {
    let Value::List(bytes) = blending
        .field_mut(BAKING_VOXEL_MATERIALS)?
        .field_mut(DATA)?
    else {
        return None;
    };
    if bytes.len() < TERRAIN_IDS * LOOK_ENTRY {
        return None;
    }
    for material in materials {
        let from = usize::from(material.like) * LOOK_ENTRY;
        let to = usize::from(material.id) * LOOK_ENTRY;
        let entry: Vec<Value> = bytes.get(from..from + LOOK_ENTRY)?.to_vec();
        bytes.get_mut(to..to + LOOK_ENTRY)?.clone_from_slice(&entry);
    }
    Some(())
}

#[cfg(windows)]
pub(crate) use running::{Builder, TerrainPatch, add, record, sealed, serve};

/// The materials in the running game: mods' registrations, the archive's
/// palette, and the loader's patches.
#[cfg(windows)]
mod running {
    use std::sync::OnceLock;

    use enshwick_base::resources::{Registry as Codec, Value};
    use enshwick_core::abi::{ResourceKey, TerrainMaterialAdd};
    use enshwick_core::content::Guid;
    use enshwick_core::typed::{
        Named, TerraformingEfficiencyRegistryResource, VoxelMaterialVfxDataResource,
        VoxelWorldResource, WorldMaterialBlending2Resource,
    };
    use enshwick_gamedata::catalogue::type_name;
    use enshwick_gamedata::reflect::fnv1a32;

    use super::{
        Material, Palette, TerrainMaterialError, add_rules, copy_effects, copy_looks, name_in,
        orphans, place,
    };
    use crate::content::served::OwnPatch;
    use crate::engine::fuse::Fuse;
    use crate::engine::loader::Loader;
    use crate::engine::registry::Registry;

    /// The materials mods added, in load order.
    static MATERIALS: Registry<Material> = Registry::new();

    /// The feature every terrain material patch's switch names.
    const FEATURE: &str = "terrain materials";

    /// The engine name and resource type hash of a resource type the loader
    /// patches.
    fn of<T: Named>() -> (&'static str, u32) {
        let name = type_name(T::ID);
        (name, fnv1a32(name.as_bytes()))
    }

    /// The game's palette and the keys of its palette worlds, read once
    /// from the archive; `None` when the archive or reflection lacks them.
    fn palette(loader: &Loader) -> Option<&'static (Palette, Vec<ResourceKey>)> {
        static PALETTE: OnceLock<Option<(Palette, Vec<ResourceKey>)>> = OnceLock::new();
        PALETTE
            .get_or_init(|| {
                let archive = crate::engine::archive::game()?;
                let codec = loader.reflection.registry();
                let read = |name: &str, key: &ResourceKey| {
                    codec.decode(name, &archive.resource(key)?).ok()
                };
                let (world, world_hash) = of::<VoxelWorldResource>();
                let worlds: Vec<(ResourceKey, Value)> = archive
                    .keys_of(world_hash)
                    .into_iter()
                    .filter_map(|key| Some((key, read(world, &key)?)))
                    .filter(|(_, value)| names_any(value))
                    .collect();
                let (rules, rules_hash) = of::<TerraformingEfficiencyRegistryResource>();
                let rules = archive
                    .keys_of(rules_hash)
                    .first()
                    .and_then(|key| read(rules, key))?;
                if worlds.is_empty() {
                    return None;
                }
                let palette = Palette::read(worlds.iter().map(|(_, value)| value), &rules)?;
                Some((palette, worlds.into_iter().map(|(key, _)| key).collect()))
            })
            .as_ref()
    }

    /// Whether the voxel world `world` names any material: a palette world,
    /// not a voxel model.
    fn names_any(world: &Value) -> bool {
        matches!(
            world.field(super::MATERIAL_GUIDS),
            Some(Value::List(guids)) if guids.iter().any(|g| matches!(g, Value::Guid(g) if *g != Guid::NIL))
        )
    }

    /// Adds the material `request` asks for and returns its terrain id for
    /// this run.
    ///
    /// # Errors
    ///
    /// [`TerrainMaterialError`] after init, without the archive's palette,
    /// for a material the game lacks, an unknown change, a GUID added before
    /// and once every free id is taken.
    pub(crate) fn add(
        loader: &Loader,
        request: &TerrainMaterialAdd,
    ) -> Result<u8, TerrainMaterialError> {
        if MATERIALS.is_sealed() {
            return Err(TerrainMaterialError::TooLate);
        }
        let (palette, _) = palette(loader).ok_or(TerrainMaterialError::Unread)?;
        MATERIALS.edit(|added| {
            let material = place(palette, added, request)?;
            added.push(material);
            Ok(material.id)
        })?
    }

    /// Seals the mods' materials: none is added after.
    pub(crate) fn sealed() -> &'static [Material] {
        MATERIALS.seal()
    }

    /// What the clone record keeps of `materials`: each GUID with its
    /// fallback's.
    pub(crate) fn record(loader: &Loader, materials: &[Material]) -> Vec<(Guid, Guid)> {
        let Some((palette, _)) = palette(loader) else {
            return Vec::new();
        };
        materials
            .iter()
            .filter_map(|m| Some((m.guid, palette.guid_of(m.fallback)?)))
            .collect()
    }

    /// Builds a served resource from its base's bytes.
    pub(crate) type Builder = Box<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync>;

    /// The loader's patch of each key the materials `added` and the orphans
    /// of `recorded` change, with its builder; none when there are none.
    pub(crate) fn serve(
        loader: &'static Loader,
        added: &[Material],
        recorded: &[(Guid, Guid)],
    ) -> Vec<(ResourceKey, Builder)> {
        let Some((palette, worlds)) = palette(loader) else {
            if !added.is_empty() || !recorded.is_empty() {
                crate::log!(
                    WARN,
                    "terrain materials off: this build's archive lacks the terrain palette"
                );
            }
            return Vec::new();
        };
        let (kept, lost) = orphans(palette, added, recorded);
        log(added, &kept, &lost);
        let materials: Vec<Material> = added.iter().chain(&kept).copied().collect();
        if materials.is_empty() {
            return Vec::new();
        }
        let Some(archive) = crate::engine::archive::game() else {
            return Vec::new();
        };
        let patch = |name: &'static str, edit: Edit| {
            let materials = materials.clone();
            let codec: Codec<'static> = loader.reflection.registry();
            Box::new(move |base: &[u8]| {
                let mut value = codec.decode(name, base).ok()?;
                edit(&mut value, &materials)?;
                codec.encode(name, &value, Some(base)).ok()
            }) as Builder
        };
        let (world, _) = of::<VoxelWorldResource>();
        let mut patches: Vec<(ResourceKey, Builder)> = worlds
            .iter()
            .map(|key| (*key, patch(world, name_in)))
            .collect();
        let tables: [((&'static str, u32), Edit); 3] = [
            (of::<TerraformingEfficiencyRegistryResource>(), add_rules),
            (of::<VoxelMaterialVfxDataResource>(), copy_effects),
            (of::<WorldMaterialBlending2Resource>(), copy_looks),
        ];
        for ((name, hash), edit) in tables {
            for key in archive.keys_of(hash) {
                patches.push((key, patch(name, edit)));
            }
        }
        patches
    }

    /// One of the edits a material makes of a resource.
    type Edit = fn(&mut Value, &[Material]) -> Option<()>;

    /// Logs once what the run's materials are.
    fn log(added: &[Material], kept: &[Material], lost: &[Guid]) {
        let named =
            |guid: Guid| crate::base::ids::content_id(guid).unwrap_or_else(|| guid.to_string());
        for m in added {
            crate::log!(
                INFO,
                "terrain material {} is terrain id {} this run, like {}",
                named(m.guid),
                m.id,
                m.like
            );
        }
        for m in kept {
            crate::log!(
                INFO,
                "terrain material {} of a removed mod loads as terrain id {} this run, a copy of its fallback {}",
                m.guid,
                m.id,
                m.fallback
            );
        }
        for guid in lost {
            crate::log!(
                WARN,
                "terrain material {guid} of a removed mod loads as air: no free terrain id is left, or the game lacks its fallback"
            );
        }
    }

    /// The loader's own patch of a resource terrain materials change.
    pub(crate) struct TerrainPatch {
        pub(crate) builder: Builder,
        /// This patch's switch alone: a palette world that panics stays
        /// vanilla, and the other keys keep their materials.
        pub(crate) fuse: Fuse,
    }

    impl TerrainPatch {
        /// The patch `builder` makes, under a switch of its own.
        pub(crate) fn new(builder: Builder) -> Self {
            Self {
                builder,
                fuse: Fuse::new(FEATURE),
            }
        }
    }

    impl OwnPatch for TerrainPatch {
        const SITE: &'static str = "a terrain material build";

        fn fuse(&self) -> &Fuse {
            &self.fuse
        }

        fn build(&self, base: Vec<u8>) -> Option<Vec<u8>> {
            (self.builder)(&base)
        }
    }
}

#[cfg(test)]
mod tests;
