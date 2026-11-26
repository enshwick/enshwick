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

use std::sync::atomic::{AtomicBool, Ordering};

use enshwick_base::resources::Value;
use enshwick_core::abi::TerrainMaterialAdd;
use enshwick_core::content::Guid;

use crate::base::join::Host;
use crate::engine::mod_index::ModIndex;
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
const CONDITIONAL_BUFFS: &str = "conditionalBuffTypes";
const MIN_SUBMERGE: &str = "minSubmergePercentage";
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
    #[cfg_attr(
        all(not(windows), test),
        expect(dead_code, reason = "only the running game reads the archive")
    )]
    Unread,
    /// Its mod does not run on the clients that join, which name terrain
    /// ids too.
    #[error("a terrain material needs a mod of side both, as clients take its id too")]
    #[cfg_attr(
        all(not(windows), test),
        expect(dead_code, reason = "only the running game knows a mod's side")
    )]
    NotBoth,
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

    /// Writes the changes into the terraforming rule `config`; `None` when
    /// it lacks a field one of them sets, so no change is lost unseen.
    fn apply(&self, config: &mut Value) -> Option<()> {
        let changes = [
            (TERRAIN_ITEM, self.item.map(Value::Guid)),
            (HARDNESS, self.hardness.map(|h| Value::Uint(u64::from(h)))),
            (HEALTH_POINTS, self.health.map(|h| Value::Int(i64::from(h)))),
            (CAN_BE_FLATTENED, self.flattens.map(Value::Bool)),
        ];
        for (name, value) in changes {
            if let Some(value) = value {
                *config.field_mut(name)? = value;
            }
        }
        Some(())
    }
}

/// A terrain material the loader serves: a mod's, or a removed mod's kept
/// as its fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Material {
    /// The mod that added it, or the loader for a removed mod's.
    owner: ModIndex,
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
    /// The palette of the palette worlds `worlds` and the rules `terraforming`:
    /// a terrain id any world names takes the first world's name for it.
    fn read<'a>(worlds: impl IntoIterator<Item = &'a Value>, terraforming: &Value) -> Option<Self> {
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
        let Some(Value::List(configs)) = terraforming.field(TERRAIN_CONFIGS) else {
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

/// The material `request` of mod `owner` asks for, placed after `added` in
/// `free`: its terrain id.
fn place(
    palette: &Palette,
    added: &[Material],
    owner: ModIndex,
    request: &TerrainMaterialAdd,
) -> Result<Material, TerrainMaterialError> {
    for id in [request.like, request.fallback] {
        if !palette.holds(id) {
            return Err(TerrainMaterialError::NotVanilla(id));
        }
    }
    let changes = Changes::of(request).ok_or(TerrainMaterialError::BadChanges)?;
    if request.guid == Guid::NIL
        || palette.id_of(request.guid).is_some()
        || added.iter().any(|m| m.guid == request.guid)
    {
        return Err(TerrainMaterialError::Taken);
    }
    let free = palette.free();
    let id = *free
        .get(added.len())
        .ok_or(TerrainMaterialError::Full(free.len()))?;
    Ok(Material {
        owner,
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
        // The fallback first: an orphan the game cannot copy takes no id.
        let fallback = palette.id_of(fallback).filter(|&id| palette.holds(id));
        match fallback.and_then(|fallback| Some((fallback, free.next()?))) {
            Some((fallback, id)) => kept.push(Material {
                owner: ModIndex::LOADER,
                guid,
                id,
                like: fallback,
                fallback,
                changes: Changes::default(),
            }),
            None => lost.push(guid),
        }
    }
    (kept, lost)
}

/// The recorded materials `recorded` with those the loaded world held last
/// time, `held`, first, so its ground wins a free id over a removed mod's
/// it never held; each part keeps its order.
fn prefer(recorded: &[(Guid, Guid)], held: &[Guid]) -> Vec<(Guid, Guid)> {
    let (first, rest): (Vec<_>, Vec<_>) = recorded.iter().partition(|(g, _)| held.contains(g));
    first.into_iter().chain(rest).collect()
}

/// What a world's record of its terrain materials holds, in its loader data
/// folder.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct Held {
    /// The GUIDs of the terrain materials served, mods' and orphans'.
    held: Vec<String>,
}

/// A world's record for a world served `materials`.
fn held(materials: &[Material]) -> Held {
    Held {
        held: materials.iter().map(|m| m.guid.to_string()).collect(),
    }
}

/// The GUIDs a world's record names; none for a file that does not read, and
/// an entry that is no GUID is skipped.
fn held_of(text: &str) -> Vec<Guid> {
    toml::from_str::<Held>(text).map_or_else(
        |_| Vec::new(),
        |held| {
            held.held
                .iter()
                .filter_map(|g| Guid::try_parse(g))
                .collect()
        },
    )
}

/// The materials `added` as served this run: the material of a mod `on`
/// does not name as a copy of its fallback, in the id the mod was told,
/// so the ids of the mods after it hold and its ground loads as an orphan's.
fn serving(added: &[Material], on: impl Fn(ModIndex) -> bool) -> Vec<Material> {
    added
        .iter()
        .map(|&m| {
            if on(m.owner) {
                m
            } else {
                Material {
                    like: m.fallback,
                    changes: Changes::default(),
                    ..m
                }
            }
        })
        .collect()
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

/// Adds one terraforming rule for each terrain id of `free` to `rules`, so
/// the rules come out one size whichever materials a session serves, as a
/// rebuild in place needs. A material of `materials` gets a copy of its
/// `like`'s rule with its changes; any other id, which no voxel holds, and a
/// material whose `like` these rules lack, a copy of the first rule.
fn add_rules(rules: &mut Value, free: &[u8], materials: &[Material]) -> Option<()> {
    let Value::List(configs) = rules.field_mut(TERRAIN_CONFIGS)? else {
        return None;
    };
    let pad = Padding::of(configs)?;
    let mut added = Vec::with_capacity(free.len());
    for &id in free {
        let material = materials.iter().find(|m| m.id == id);
        let like = material
            .and_then(|m| configs.iter().find(|c| config_id(c) == Some(m.like)))
            .or_else(|| configs.first())?;
        let mut config = like.clone();
        *config
            .field_mut(TERRAIN_MATERIAL)?
            .field_mut(MATERIAL_ID_VALUE)? = Value::Uint(u64::from(id));
        if let Some(material) = material {
            material.changes.apply(&mut config)?;
        }
        pad.fill(&mut config)?;
        added.push(config);
    }
    configs.extend(added);
    Some(())
}

/// How the loader's rules pad their conditional buffs to one count, the
/// most a game rule holds (2 on client 26430625: ids 16 and 51), since a rule
/// copied from one with fewer would make the rules smaller.
struct Padding {
    most: usize,
    /// A game rule's conditional buff whose submerge fraction no voxel
    /// reaches, so it never applies (fractions 0.25 and 0.9 on client
    /// 26430625).
    never: Option<Value>,
}

impl Padding {
    /// The padding of the game's rules `configs`; `None` when they hold a
    /// conditional buff without the submerge fraction, so no buff can be
    /// made that never applies.
    fn of(configs: &[Value]) -> Option<Self> {
        let lists = || {
            configs
                .iter()
                .filter_map(|c| match c.field(CONDITIONAL_BUFFS) {
                    Some(Value::List(list)) => Some(list),
                    _ => None,
                })
        };
        let never = match lists().flatten().next() {
            None => None,
            Some(entry) => {
                let mut entry = entry.clone();
                *entry.field_mut(MIN_SUBMERGE)? = Value::Float(f64::INFINITY);
                Some(entry)
            }
        };
        Some(Self {
            most: lists().map(Vec::len).max().unwrap_or(0),
            never,
        })
    }

    /// Pads the conditional buffs of `config` to the count; `None` when it
    /// lacks them while game rules hold some.
    fn fill(&self, config: &mut Value) -> Option<()> {
        let Some(never) = &self.never else {
            return Some(());
        };
        let Value::List(list) = config.field_mut(CONDITIONAL_BUFFS)? else {
            return None;
        };
        list.resize(self.most.max(list.len()), never.clone());
        Some(())
    }
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

/// What the player reads when `shifted`, a mod still on, reads another
/// terrain id than `host` serves, in the reader's language: `extra` are the
/// mods turned off for this game whose terrain moved it.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "a client joins only in the game")
)]
fn moved_text(host: Host, shifted: &str, extra: &[String]) -> String {
    use crate::engine::texts;
    let shifted = [("mod", shifted)];
    if extra.is_empty() {
        return match host {
            Host::Server => texts::fill("join.terrain.server.moved", &shifted),
            Host::Player => texts::fill("join.terrain.host.moved", &shifted),
        };
    }
    let (head, tail) = match host {
        Host::Server => (
            texts::fill("join.terrain.server.extra", &[]),
            texts::fill("join.terrain.server.shifted", &shifted),
        ),
        Host::Player => (
            texts::fill("join.terrain.host.extra", &[]),
            texts::fill("join.terrain.host.shifted", &shifted),
        ),
    };
    format!("{head}\n\n{}\n\n{tail}", extra.join("\n"))
}

/// What the player reads when the terrain ids `host` serves do not read on
/// this client, or its served resources did not all take them.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "a client joins only in the game")
)]
fn untaken_text(host: Host) -> String {
    match host {
        Host::Server => crate::engine::texts::fill("join.terrain.server.untaken", &[]),
        Host::Player => crate::engine::texts::fill("join.terrain.host.untaken", &[]),
    }
}

/// The mods whose terrain moved `shifted`'s id: of `added`, in placement
/// order, the owners before `shifted`'s first material that are `off` for
/// this game.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "a client joins only in the game")
)]
fn moved_by(
    added: &[Material],
    shifted: ModIndex,
    off: impl Fn(ModIndex) -> bool,
) -> Vec<ModIndex> {
    let before = added
        .iter()
        .position(|m| m.owner == shifted)
        .unwrap_or(added.len());
    let mut owners: Vec<ModIndex> = added[..before]
        .iter()
        .map(|m| m.owner)
        .filter(|&owner| off(owner))
        .collect();
    owners.dedup();
    owners
}

/// Whether terrain id `id` holds only a padding rule of [`add_rules`]: one
/// of the `free` ids no material of the served `table` takes, which no voxel
/// holds.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "terrain is served only in the game")
)]
fn pads(free: &[u8], table: &[Material], id: u8) -> bool {
    free.contains(&id) && !table.iter().any(|m| m.id == id)
}

/// The field of the server's configuration that names its save folder.
const SAVE_DIRECTORY: &str = "saveDirectory";
/// The save folder a configuration without [`SAVE_DIRECTORY`] gets.
const DEFAULT_SAVES: &str = "./savegame";

/// The save folder the server's configuration `config` names, as the game
/// resolves it: a relative one from the working directory, as the world
/// keys the records are written under.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only the running server reads its configuration")
)]
fn saves_folder(config: Option<&str>) -> std::path::PathBuf {
    config
        .and_then(|json| crate::base::advertise::configured(json, SAVE_DIRECTORY))
        .unwrap_or_else(|| DEFAULT_SAVES.to_owned())
        .into()
}

/// The table the served terrain resources build from.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "terrain is served only in the game")
)]
enum Session {
    /// None yet: the first build places this side's own, so the loaded
    /// world's orphans can go first.
    #[default]
    Unplaced,
    /// This side's own.
    Own(Vec<Material>),
    /// A host's, while this client joins its game.
    Host(Vec<Material>),
}

#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "terrain is served only in the game")
)]
impl Session {
    /// The table the served resources were built from, `None` before one.
    fn table(&self) -> Option<&[Material]> {
        match self {
            Self::Unplaced => None,
            Self::Own(table) | Self::Host(table) => Some(table),
        }
    }

    /// Whether serving `next` means building the served resources again:
    /// they were built from another table.
    fn rebuilds_for(&self, next: &[Material]) -> bool {
        self.table().is_some_and(|now| now != next)
    }
}

/// Whether the host's table served now is the one of a game the client
/// plays or played: set when play starts with a host's table, cleared by
/// every switch of the table, so a join taken while still in a game keeps
/// the table it took when that game ends.
#[derive(Debug)]
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "terrain is served only in the game")
)]
struct Played(AtomicBool);

#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "terrain is served only in the game")
)]
impl Played {
    const fn new() -> Self {
        Self(AtomicBool::new(false))
    }

    /// The served table changed.
    fn switched(&self) {
        self.0.store(false, Ordering::Release);
    }

    /// Play started (`playing`) or ended; `host` answers, only at a start,
    /// whether the table served is a host's. Whether play ended in a game
    /// played with a host's table, once per such game.
    fn changed(&self, playing: bool, host: impl FnOnce() -> bool) -> bool {
        if !playing {
            return self.0.swap(false, Ordering::AcqRel);
        }
        if host() {
            self.0.store(true, Ordering::Release);
        }
        false
    }
}

/// The feature every terrain material patch's switch names.
#[cfg(windows)]
pub(crate) const FEATURE: &str = "terrain materials";

#[cfg(windows)]
pub(crate) use running::{add, in_time, padding, record, sealed, serve, world_event};

/// The materials in the running game: mods' registrations, the archive's
/// palette, the loader's patches and the table a session serves.
#[cfg(windows)]
mod running;

pub(crate) mod table;

#[cfg(test)]
mod tests;
