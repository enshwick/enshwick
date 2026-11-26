use std::sync::{Arc, Mutex, OnceLock};

use enshwick_base::Relock;
use enshwick_base::resources::{Registry as Codec, Value};
use enshwick_core::abi::{Event, ResourceKey, Side, TerrainMaterialAdd};
use enshwick_core::content::Guid;
use enshwick_core::typed::{
    Named, TerraformingEfficiencyRegistryResource, VoxelMaterialVfxDataResource,
    VoxelWorldResource, WorldMaterialBlending2Resource,
};
use enshwick_gamedata::catalogue::type_name;
use enshwick_gamedata::reflect::fnv1a32;

use super::table::{self, Adopted};
use super::{
    Host, Material, Palette, Played, Session, TerrainMaterialError, add_rules, copy_effects,
    copy_looks, held, held_of, moved_by, moved_text, name_in, orphans, pads, place, prefer,
    saves_folder, serving, untaken_text,
};
use crate::content::served::{Builder, mod_on};
use crate::engine::fuse::Fuse;
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use crate::engine::registry::Registry;

/// The terrain materials of the run.
struct Terrain {
    /// The materials mods added, in load order.
    materials: Registry<Material>,
    /// The game's palette and the keys of its palette worlds, read once
    /// from the archive; `None` when the archive or reflection lacks them.
    palette: OnceLock<Option<(Palette, Vec<ResourceKey>)>>,
    /// What this side serves of its own, set at the seal when it serves
    /// terrain at all.
    own: OnceLock<Own>,
    /// The table the served terrain resources build from.
    session: Mutex<Session>,
    /// Whether the host's table served is the one of a game played.
    played: Played,
}

/// Taking a host's terrain ids at a join: a panic there turns off only
/// that, so the join check and the lobby go on.
static HOST_IDS_FUSE: Fuse = Fuse::new("terrain ids from the host");
/// Serving this side's own terrain ids again: apart from taking a host's,
/// so a panic there leaves the way back on; while it is off, no host's
/// table is taken, since none could be left.
static OWN_IDS_FUSE: Fuse = Fuse::new("own terrain ids");

static TERRAIN: Terrain = Terrain {
    materials: Registry::new(),
    palette: OnceLock::new(),
    own: OnceLock::new(),
    session: Mutex::new(Session::Unplaced),
    played: Played::new(),
};

/// What this side serves of its own.
struct Own {
    loader: &'static Loader,
    /// The mods' materials, in load order, with the ids they were told.
    added: &'static [Material],
    /// The clone record's materials, each GUID with its fallback's.
    recorded: Vec<(Guid, Guid)>,
    /// The keys the loader serves terrain on, built again when the table
    /// changes.
    keys: Vec<ResourceKey>,
}

/// The engine name and resource type hash of a resource type the loader
/// patches.
fn of<T: Named>() -> (&'static str, u32) {
    let name = type_name(T::ID);
    (name, fnv1a32(name.as_bytes()))
}

/// The game's palette and the keys of its palette worlds; `None` when the
/// archive or reflection lacks them.
fn palette(loader: &Loader) -> Option<&'static (Palette, Vec<ResourceKey>)> {
    TERRAIN
        .palette
        .get_or_init(|| {
            let archive = crate::engine::archive::game()?;
            let codec = loader.reflection.registry();
            let read =
                |name: &str, key: &ResourceKey| codec.decode(name, &archive.resource(key)?).ok();
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

/// Whether the voxel world `world` names any material: a palette world, not
/// a voxel model.
fn names_any(world: &Value) -> bool {
    matches!(
        world.field(super::MATERIAL_GUIDS),
        Some(Value::List(guids)) if guids.iter().any(|g| matches!(g, Value::Guid(g) if *g != Guid::NIL))
    )
}

/// Adds the material `request` of mod `owner` asks for and returns its
/// terrain id for this run; `owner` pairs the mod with whether it runs on
/// the clients that join.
///
/// # Errors
///
/// [`TerrainMaterialError`] after init, without the archive's palette, for
/// a mod clients do not run, a material the game lacks, an unknown change,
/// a GUID the game or a mod named before and once every free id is taken.
pub(crate) fn add(
    loader: &Loader,
    (owner, joins): (ModIndex, bool),
    request: &TerrainMaterialAdd,
) -> Result<u8, TerrainMaterialError> {
    let (palette, _) = palette(loader).ok_or(TerrainMaterialError::Unread)?;
    // A joining client takes the host's ids, which name only the host's
    // materials, so a material clients lack would have none there.
    if !joins {
        return Err(TerrainMaterialError::NotBoth);
    }
    TERRAIN.materials.edit(|added| {
        let material = place(palette, added, owner, request)?;
        added.push(material);
        Ok(material.id)
    })?
}

/// Whether mods may still add terrain materials: the host entry asks it
/// before the other refusals, so a late call reads as late.
///
/// # Errors
///
/// [`TerrainMaterialError::TooLate`] after init.
pub(crate) fn in_time() -> Result<(), TerrainMaterialError> {
    if TERRAIN.materials.is_sealed() {
        return Err(TerrainMaterialError::TooLate);
    }
    Ok(())
}

/// Seals the mods' materials: none is added after.
pub(crate) fn sealed() -> &'static [Material] {
    TERRAIN.materials.seal()
}

/// What the clone record keeps of `materials`: each GUID with its
/// fallback's.
pub(crate) fn record(loader: &Loader, materials: &[Material]) -> Vec<(Guid, Guid)> {
    if materials.is_empty() {
        return Vec::new();
    }
    let Some((palette, _)) = palette(loader) else {
        return Vec::new();
    };
    materials
        .iter()
        .filter_map(|m| Some((m.guid, palette.guid_of(m.fallback)?)))
        .collect()
}

/// The loader's patch of each key terrain materials change, with its
/// builder. A client serves them whenever the archive has the palette, so
/// it can take a host's table, and the patches keep the resources one size
/// for a rebuild in place; a dedicated server, which never takes one, only
/// with the mods' materials `added` or the orphans of `recorded`.
pub(crate) fn serve(
    loader: &'static Loader,
    added: &'static [Material],
    recorded: &[(Guid, Guid)],
) -> Vec<(ResourceKey, Builder)> {
    let none = added.is_empty() && recorded.is_empty();
    if none && loader.side == Side::SERVER {
        return Vec::new();
    }
    let Some((palette, worlds)) = palette(loader) else {
        if !none {
            crate::log!(
                WARN,
                "terrain materials off: this build's archive lacks the terrain palette"
            );
        }
        return Vec::new();
    };
    let Some(archive) = crate::engine::archive::game() else {
        return Vec::new();
    };
    // `always_edits`: the edit changes the resource even for an empty table.
    let patch = |name: &'static str, edit: &Edit, always_edits: bool| {
        let edit = Arc::clone(edit);
        let codec: Codec<'static> = loader.reflection.registry();
        Box::new(move |base: Vec<u8>| {
            let table = session();
            // The codec writes a resource back to the bytes it read (every
            // resource of client and server data 1287369), so an empty table
            // builds the base: no decode for a client without terrain.
            if table.is_empty() && !always_edits {
                return Some(base);
            }
            let mut value = codec.decode(name, &base).ok()?;
            edit(&mut value, &table)?;
            codec.encode(name, &value, Some(&base)).ok()
        }) as Builder
    };
    let (world, _) = of::<VoxelWorldResource>();
    let names: Edit = Arc::new(name_in);
    let mut patches: Vec<(ResourceKey, Builder)> = worlds
        .iter()
        .map(|key| (*key, patch(world, &names, false)))
        .collect();
    let free = palette.free();
    let tables: [((&'static str, u32), Edit, bool); 3] = [
        (
            of::<TerraformingEfficiencyRegistryResource>(),
            Arc::new(move |rules: &mut Value, materials: &[Material]| {
                add_rules(rules, &free, materials)
            }),
            true,
        ),
        (
            of::<VoxelMaterialVfxDataResource>(),
            Arc::new(copy_effects),
            false,
        ),
        (
            of::<WorldMaterialBlending2Resource>(),
            Arc::new(copy_looks),
            false,
        ),
    ];
    for ((name, hash), edit, always_edits) in &tables {
        for key in archive.keys_of(*hash) {
            patches.push((key, patch(name, edit, *always_edits)));
        }
    }
    let _first = TERRAIN.own.set(Own {
        loader,
        added,
        recorded: recorded.to_vec(),
        keys: patches.iter().map(|(key, _)| *key).collect(),
    });
    crate::base::join::set_terrain_ids(crate::base::join::TerrainIds {
        served: || served_text().unwrap_or_default(),
        adopt: |table, host| {
            let adopted = (!OWN_IDS_FUSE.is_off())
                .then(|| HOST_IDS_FUSE.run("the host's terrain ids", || adopt_host(table)))
                .flatten();
            match adopted {
                Some(mismatch) => mismatch.map(|m| m.text(host)),
                // Off or panicked: whatever it served, it did not take the host's.
                None => Some(untaken_text(host)),
            }
        },
        own_again: own_again_fused,
        play_changed: |playing| {
            TERRAIN.played.changed(playing, || {
                matches!(*TERRAIN.session.relock(), Session::Host(_))
            })
        },
    });
    crate::engine::world::on_hosted_start(crate::base::join::hosting_starts);
    patches
}

/// One of the edits a session's table makes of a resource.
type Edit = Arc<dyn Fn(&mut Value, &[Material]) -> Option<()> + Send + Sync>;

/// This side's own table now: the mods' materials as served, the material
/// of a mod turned off as a copy of its fallback, then the recorded orphans,
/// those the loaded world held first. With them, the orphans left out.
fn own_table(own: &Own) -> Option<(Vec<Material>, Vec<Material>, Vec<Guid>)> {
    let (palette, _) = palette(own.loader)?;
    let added = serving(own.added, mod_on);
    let held = world_held(own.loader);
    let (kept, lost) = orphans(palette, &added, &prefer(&own.recorded, &held));
    Some((added, kept, lost))
}

/// The table the served terrain resources build from now, placing this
/// side's own when none is placed yet.
fn session() -> Vec<Material> {
    let mut session = TERRAIN.session.relock();
    if let Some(table) = session.table() {
        return table.to_vec();
    }
    let Some(table) = TERRAIN.own.get().and_then(placed) else {
        return Vec::new();
    };
    *session = Session::Own(table.clone());
    table
}

/// This side's own table, placed now and logged.
fn placed(own: &Own) -> Option<Vec<Material>> {
    let (added, kept, lost) = own_table(own)?;
    log(&added, &kept, &lost);
    Some(added.into_iter().chain(kept).collect())
}

/// Serves `next`: every terrain key the game built is built again in place
/// when it was built from another table. Whether every key took it.
fn switch(own: &Own, next: Session) -> bool {
    let rebuilds = {
        let mut session = TERRAIN.session.relock();
        let rebuilds = next
            .table()
            .is_some_and(|table| session.rebuilds_for(table));
        TERRAIN.played.switched();
        *session = next;
        rebuilds
    };
    if !rebuilds {
        return true;
    }
    let mut took = true;
    for key in &own.keys {
        if let Err(why) = crate::content::served::rebuild(ModIndex::LOADER, *key) {
            took = false;
            crate::log!(
                WARN,
                "terrain resource {:08x}:{} keeps the last session's ids: {why}",
                key.type_hash,
                key.guid
            );
        }
    }
    took
}

/// [`own_again`] under its own switch, on every path that serves this
/// side's own table again, the join check's included.
fn own_again_fused() {
    let _ran = OWN_IDS_FUSE.run("the client's own terrain ids", own_again);
}

/// Serves this side's own table again, once a game played with a host's
/// table ends, before a game this client hosts starts or a join that
/// brings no table: a host's table of a game it left goes. Before any build
/// the first one places it.
fn own_again() {
    let Some(own) = TERRAIN.own.get() else {
        return;
    };
    if *TERRAIN.session.relock() == Session::Unplaced {
        return;
    }
    if let Some(table) = placed(own) {
        switch(own, Session::Own(table));
    }
}

/// Why this client's terrain ids do not match its host's this game.
#[derive(Debug)]
enum Mismatch {
    /// The host's table does not read here, or a served resource did not
    /// take it.
    Untaken,
    /// `shifted`, a mod still on, reads another id than the host serves;
    /// the mods turned off for this game in `extra` moved it.
    Moved {
        shifted: ModIndex,
        extra: Vec<ModIndex>,
    },
}

impl Mismatch {
    /// What the player reads of it in a game `host` hosts.
    fn text(&self, host: Host) -> String {
        let Self::Moved { shifted, extra } = self else {
            return untaken_text(host);
        };
        let names = crate::base::join::names();
        let label = |index: ModIndex| {
            crate::engine::loader::loader()
                .and_then(|loader| loader.mods.get(index.get()))
                .map_or_else(String::new, |m| {
                    enshwick_core::mods::label(names.get(&m.id).map_or("", String::as_str), &m.id)
                })
        };
        let extra: Vec<String> = extra.iter().map(|&m| label(m)).collect();
        moved_text(host, &label(*shifted), &extra)
    }
}

/// Takes the terrain table `host` of the game this client joins, as the
/// host's rules or lobby give it, before the game connects; `None` keeps
/// this side's own. The mismatch, when the table does not read or take, or
/// a mod still on reads a wrong id this session.
fn adopt_host(host: Option<&str>) -> Option<Mismatch> {
    let own = TERRAIN.own.get()?;
    let Some(text) = host else {
        own_again_fused();
        return None;
    };
    let (palette, _) = palette(own.loader)?;
    let Some(entries) = table::parse(text, palette) else {
        crate::log!(WARN, "the host's terrain table does not read: {text}");
        own_again_fused();
        return Some(Mismatch::Untaken);
    };
    let Adopted { table, moved } = table::adopt(own.added, &entries, mod_on);
    crate::log!(INFO, "terrain ids from the host: {text}");
    if !switch(own, Session::Host(table)) {
        return Some(Mismatch::Untaken);
    }
    let shifted = moved?;
    let extra = moved_by(own.added, shifted, crate::base::join::client::off_for_world);
    Some(Mismatch::Moved { shifted, extra })
}

/// Whether terrain id `id` holds only a padding rule the loader serves this
/// session, which reads of the live rules leave out.
pub(crate) fn padding(id: u8) -> bool {
    let Some(own) = TERRAIN.own.get() else {
        return false;
    };
    palette(own.loader).is_some_and(|(palette, _)| pads(&palette.free(), &session(), id))
}

/// The text of this side's own table, for the clients that join it; `None`
/// when it serves no terrain. A host's table is never passed on: a client
/// that hosts serves its own from the start.
fn served_text() -> Option<String> {
    let own = TERRAIN.own.get()?;
    if let Session::Own(table) = &*TERRAIN.session.relock() {
        return Some(table::text(table));
    }
    let (added, kept, _) = own_table(own)?;
    Some(table::text(&[added, kept].concat()))
}

/// At a world's load, records beside its save the materials it was served,
/// so next time its orphans take a free id before others.
pub(crate) fn world_event(loader: &Loader, event: Event) {
    if event != Event::WORLD_LOADED || TERRAIN.own.get().is_none() {
        return;
    }
    let Some(key) = crate::base::saves::world_key(loader) else {
        return;
    };
    let path = crate::base::world_folders::loader_data(key, HELD_FILE);
    if let Err(error) = crate::base::world_folders::write_toml(&path, "", &held(&session())) {
        crate::log!(
            WARN,
            "the terrain materials of world {key} were not recorded: {error}"
        );
    }
}

/// The terrain materials the loaded world was served when it last ran. A
/// dedicated server names its world only after its palette builds (server
/// 26430253: 51 ms later), but runs a world of its one save folder, so it
/// takes what every world there held.
fn world_held(loader: &Loader) -> Vec<Guid> {
    let read = |path: std::path::PathBuf| {
        std::fs::read_to_string(path).map_or_else(|_| Vec::new(), |text| held_of(&text))
    };
    if let Some(key) = crate::base::saves::world_key(loader) {
        return read(crate::base::world_folders::loader_data(key, HELD_FILE));
    }
    if loader.side != Side::SERVER {
        return Vec::new();
    }
    let Some(game_dir) = loader.mods_dir.parent() else {
        return Vec::new();
    };
    let config = std::fs::read_to_string(game_dir.join(crate::base::advertise::SERVER_CONFIG)).ok();
    let Ok(entries) = std::fs::read_dir(saves_folder(config.as_deref())) else {
        return Vec::new();
    };
    let mut held: Vec<Guid> = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        if !name
            .to_string_lossy()
            .ends_with(crate::base::world_folders::DATA_SUFFIX)
        {
            continue;
        }
        let path = entry
            .path()
            .join(crate::engine::settings::LOADER_ID)
            .join(HELD_FILE);
        for guid in read(path) {
            if !held.contains(&guid) {
                held.push(guid);
            }
        }
    }
    held
}

/// The file in a world's loader data folder that names the terrain materials
/// it was served when it last loaded.
const HELD_FILE: &str = "terrain.toml";

/// Logs what a session's own materials are.
fn log(added: &[Material], kept: &[Material], lost: &[Guid]) {
    let named = |guid: Guid| crate::base::ids::content_id(guid).unwrap_or_else(|| guid.to_string());
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
