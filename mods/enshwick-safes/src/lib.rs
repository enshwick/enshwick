//! The Safe: a chest crafted beside the wooden chest that only the player who
//! placed it, and the players they allow with `/safe allow`, can open or
//! empty; only the owner picks it up, and nobody upgrades it away. The owner
//! shows on hover, as the chest's name.
//!
//! The Safe item places the vanilla wooden chest, so a Safe without the mod is
//! an ordinary chest with every item still in it. The placed chest keeps the
//! Safe's item id in the save, which tells the mod which chests are Safes;
//! owners and the players they allowed are kept per world, in
//! `mod-data/safes/worlds/<world>/owners.toml`, by position.
//!
//! The refusals run on the dedicated server and, on a client, in the built-in
//! server while it hosts a game; the client's own world is left alone.

pub mod access;
pub mod command;
pub mod content;

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use enshwick_loader_api::abi::BuildingData;
use enshwick_loader_api::abi::Side;
use enshwick_loader_api::commands::Rename;
use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::events::{
    BuildingPlaced, BuildingRemoved, PlayerJoined, PlayerLeft, ShutdownStarted, Ticked, WorldLoaded,
};
use enshwick_loader_api::i18n::Language;
use enshwick_loader_api::state::{Store, WorldStore};
use enshwick_loader_api::world::{ComponentType, World};
use enshwick_loader_api::{Load, Mod, Off};

use crate::access::{ACCESS, Action, Allowed, Cell, Safe, Seen, permits};
use crate::command::Phrases;

/// Safe settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(default, deny_unknown_fields)]
#[config(validate = Settings::check)]
struct Settings {
    /// The Safe costs the wooden chest's ingredients times this. A joined
    /// player's crafting menu shows the server's value.
    #[config(sync, range = 1..=100)]
    cost_multiplier: u32,
    /// Whether the server's administrators may open, remove and rename every Safe.
    #[config(runtime)]
    admins_can_open: bool,
    /// The language of the owner's name on a Safe and of `/safe` replies: "en", "uk", "de", ...
    #[config(runtime)]
    label_language: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cost_multiplier: 2,
            admins_can_open: true,
            label_language: "en".into(),
        }
    }
}

impl Settings {
    fn check(&self) -> Result<(), String> {
        self.language().map(|_| ()).ok_or_else(|| {
            format!(
                "label_language {:?} is no language code",
                self.label_language
            )
        })
    }

    fn language(&self) -> Option<Language> {
        Language::from_code(&self.label_language)
    }
}

/// The Safe's cost, times the wooden chest's, from the settings in force.
static COST: AtomicU32 = AtomicU32::new(2);

/// Puts `cost` in force, building the recipes again when it changed.
fn set_cost(cost: u32) {
    if COST.swap(cost, Ordering::Relaxed) == cost {
        return;
    }
    match enshwick_loader_api::content::recipes::rebuild_recipes() {
        Ok(()) => enshwick_loader_api::info!("the Safe costs the wooden chest x{cost}"),
        Err(off) => {
            enshwick_loader_api::warn!("the Safe's cost x{cost} waits for a restart: {off}");
        }
    }
}

/// The owners as stored: one record per Safe.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize)]
struct Owners {
    safes: Vec<Record>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Record {
    /// Half-metre cell of the Safe.
    cell: [i64; 3],
    /// The owner's account key, in hex: TOML integers stop at i64.
    owner: String,
    /// The owner's name as last seen; absent in files from 0.1.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    name: String,
    /// Players the owner allowed; absent in files from 0.1.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    allowed: Vec<AllowedRecord>,
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct AllowedRecord {
    /// The player's account key, in hex.
    key: String,
    /// Their name when they were allowed.
    name: String,
}

fn hex(key: u64) -> String {
    format!("{key:016x}")
}

impl Owners {
    fn from_map(owners: &HashMap<Cell, Safe>) -> Self {
        let mut safes: Vec<Record> = owners
            .iter()
            .map(|(&(x, y, z), safe)| Record {
                cell: [x, y, z],
                owner: hex(safe.owner),
                name: safe.owner_name.clone(),
                allowed: safe
                    .allowed
                    .iter()
                    .map(|a| AllowedRecord {
                        key: hex(a.key),
                        name: a.name.clone(),
                    })
                    .collect(),
            })
            .collect();
        safes.sort_by_key(|r| r.cell);
        Self { safes }
    }

    fn to_map(&self) -> HashMap<Cell, Safe> {
        self.safes
            .iter()
            .filter_map(|r| {
                let safe = Safe {
                    owner: u64::from_str_radix(&r.owner, 16).ok()?,
                    owner_name: r.name.clone(),
                    allowed: r
                        .allowed
                        .iter()
                        .filter_map(|a| {
                            Some(Allowed {
                                key: u64::from_str_radix(&a.key, 16).ok()?,
                                name: a.name.clone(),
                            })
                        })
                        .collect(),
                };
                Some(((r.cell[0], r.cell[1], r.cell[2]), safe))
            })
            .collect()
    }
}

/// Hands the owners to the running world's store, which writes them when
/// the world saves; false while no world is open.
fn hand_over(store: &WorldStore<Owners>) -> bool {
    let owners = Owners::from_map(&ACCESS.owners.load());
    if store.update(|kept| *kept = owners).is_none() {
        enshwick_loader_api::warn!("owners not saved: no world is open");
        return false;
    }
    true
}

/// Hands the owners to the running world's store and writes them on a
/// thread of their own; a write that finds nothing changed since the last
/// one skips, so the newest owners are always the ones on disk.
fn save(store: &'static WorldStore<Owners>) {
    if !hand_over(store) {
        return;
    }
    let spawned = std::thread::Builder::new()
        .name("safes-save".into())
        .spawn(move || {
            if let Err(error) = store.flush() {
                enshwick_loader_api::warn!("owners not saved yet: {error}");
            }
        });
    if spawned.is_err() {
        enshwick_loader_api::warn!("owners saved with the world: no thread");
    }
}

/// Puts the running world's owners in force for the checks. A world
/// without owners of its own takes those of the mod-wide `owners.toml`
/// written before owners were kept per world, once; the file is then
/// renamed out of the way.
fn open_world(store: &WorldStore<Owners>, dir: &Path) {
    let mut owners = store.with(Owners::to_map).unwrap_or_default();
    let shared = dir.join("owners.toml");
    if owners.is_empty() && store.key().is_some() && shared.exists() {
        match Store::<Owners>::new(dir, "owners").load() {
            Ok(old) => {
                owners = old.to_map();
                store.update(|kept| *kept = old);
                let moved = store.flush().map_err(|e| e.to_string()).and_then(|()| {
                    std::fs::rename(&shared, dir.join("owners.toml.moved"))
                        .map_err(|e| e.to_string())
                });
                match moved {
                    Ok(()) => enshwick_loader_api::info!(
                        "{} Safe owners moved from owners.toml into this world",
                        owners.len()
                    ),
                    Err(why) => enshwick_loader_api::warn!("owners.toml stays: {why}"),
                }
            }
            Err(error) => enshwick_loader_api::warn!("owners.toml not read: {error}"),
        }
    }
    enshwick_loader_api::info!(
        "{} Safe owners in world {}",
        owners.len(),
        store.key().unwrap_or_default()
    );
    ACCESS.forget_session();
    ACCESS.load_owners(owners);
}

/// The mod's state.
#[derive(Debug)]
pub struct Safes;

impl Mod for Safes {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "safes",
            side: BOTH,
            name: "Safes",
            join = true,
            updates: github("themaiby/enshwick", "enshwick-safes"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        load.migrate(content::migrations())?;
        let settings: Settings = load.config()?;
        let cost = settings.cost_multiplier;
        COST.store(cost, Ordering::Relaxed);
        let texts = load.texts(content::TEXTS)?;
        load.add_texts(
            &texts,
            &[
                ("safe.name", content::SAFE_NAME),
                ("safe.description", content::SAFE_DESCRIPTION),
            ],
        )?;
        load.add_item(content::item())?;
        load.add_recipe_with(|| content::recipe(COST.load(Ordering::Relaxed)))?;
        ACCESS
            .admins_open
            .store(settings.admins_can_open, Ordering::Relaxed);
        let phrases: &'static Phrases = Box::leak(Box::new(Phrases::new(
            texts,
            settings.language().unwrap_or(Language::FALLBACK),
        )));
        guard(load, phrases)?;
        enshwick_loader_api::info!(
            "the Safe costs the wooden chest x{cost}; administrators may open Safes: {}",
            settings.admins_can_open
        );
        Ok(Self)
    }
}

/// The server half: owners, players, the refusals, labels and `/safe`.
fn guard(load: &mut Load<'_>, phrases: &'static Phrases) -> Result<(), Off> {
    let world = load.world()?;
    let client = load.host().build().side == Side::CLIENT;
    let dir = load.data_dir()?;
    // The store opens on the world's load, before the handler below runs.
    let store: &'static WorldStore<Owners> =
        Box::leak(Box::new(load.world_store::<Owners>("owners")?));
    load.on::<WorldLoaded>(move |()| open_world(store, &dir))?;

    load.on_interaction(access::filter)?;
    track_players(load, world)?;
    load.on::<BuildingPlaced>(move |placed| {
        if content::is_safe(placed.item) {
            placed_safe(world, phrases, placed);
            save(store);
        }
    })?;
    load.on::<BuildingRemoved>(move |removed| {
        if ACCESS.removed(removed.entity) {
            enshwick_loader_api::info!("Safe {} removed", removed.entity);
            // Written with the world: a game that stops before it saves
            // brings the Safe back, and its owner must come back with it.
            let _open = hand_over(store);
        }
    })?;
    if client {
        // Entities, players and owners belong to one hosted game's world.
        load.on::<ShutdownStarted>(|()| {
            ACCESS.forget_session();
            ACCESS.load_owners(HashMap::new());
        })?;
        enshwick_loader_api::info!("the Safe's checks act while this client hosts a game");
    }
    load.on_config_change(move |settings: Settings| {
        ACCESS
            .admins_open
            .store(settings.admins_can_open, Ordering::Relaxed);
        phrases.set_language(settings.language().unwrap_or(Language::FALLBACK));
        set_cost(settings.cost_multiplier);
    })?;
    label_seen(load, world, phrases, store)?;
    load.on_rename(move |rename| may_rename(world, rename))?;
    load.add_command(
        "safe",
        &phrases.say("command.help", &[]),
        move |ctx, args| command::run(&world, phrases, ctx, args, || save(store)),
    )?;
    Ok(())
}

/// Records a Safe just placed, with its owner, and names the owner on it.
fn placed_safe(world: World, phrases: &Phrases, placed: &BuildingData) {
    let entity = Entity(placed.player);
    let player = world.player(entity);
    let key = player
        .as_ref()
        .map(|p| p.key)
        .or_else(|| ACCESS.players.load().get(&entity.0).map(|seen| seen.key));
    let Some(key) = key else {
        enshwick_loader_api::warn!("a Safe was placed by no known player and stays open to all");
        return;
    };
    let name = player.map(|p| p.name).unwrap_or_default();
    let position = placed.position.map(f64::from);
    ACCESS.placed(placed.entity, access::cell(position), key, &name);
    if !name.is_empty() {
        let label = phrases.say("safe.label", &[("owner", &name)]);
        if !world.set_label(Entity(placed.entity), &label) {
            enshwick_loader_api::warn!("Safe {} not labelled", placed.entity);
        }
    }
    enshwick_loader_api::info!(
        "Safe {} placed at {position:?} by {key:016x}",
        placed.entity
    );
}

/// Whether the player behind `rename` may rename its entity: anyone for
/// what is not a Safe; the owner, or an administrator when allowed, for a
/// Safe.
fn may_rename(world: World, rename: &Rename) -> bool {
    let known = ACCESS.safes.load().get(&rename.entity.0).copied();
    let found = known.or_else(|| {
        if !content::is_safe(world.used_item(rename.entity)?) {
            return None;
        }
        ACCESS.seen(
            rename.entity.0,
            access::cell(world.position(rename.entity)?),
        )
    });
    let Some((owner, at)) = found else {
        return true;
    };
    let Some(player) = &rename.player else {
        return false;
    };
    let seen = Seen {
        key: player.key,
        admin: player.is_admin(),
    };
    let admins_open = ACCESS.admins_open.load(Ordering::Relaxed);
    let owners = ACCESS.owners.load();
    let fallback = Safe::new(owner, "");
    let safe = owners.get(&at).unwrap_or(&fallback);
    let allowed = permits(safe, Some(seen), admins_open, Action::Manage);
    if !allowed {
        enshwick_loader_api::info!("{} may not rename Safe {}", player.name, rename.entity.0);
    }
    allowed
}

/// Learns every Safe the world holds that this session has not seen yet:
/// base stock asks without the chest's item or position, so a Safe nobody
/// looked at would otherwise lend its stock to anyone crafting nearby.
fn learn_safes(world: World, containers: ComponentType) {
    let known = ACCESS.safes.load();
    for placed in world.entities(containers) {
        if known.contains_key(&placed.entity.0)
            || !world.used_item(placed.entity).is_some_and(content::is_safe)
        {
            continue;
        }
        let _owner = ACCESS.seen(placed.entity.0, access::cell(placed.position));
    }
}

/// Names the owner on every Safe seen this session that has no name yet,
/// on the game thread; the owner's name comes from the record, or from the
/// owner when they are online. Learns the world's Safes first.
fn label_seen(
    load: &mut Load<'_>,
    world: World,
    phrases: &'static Phrases,
    store: &'static WorldStore<Owners>,
) -> Result<(), Off> {
    let mut done: HashSet<u32> = HashSet::new();
    let containers = world.component_type(load, "keen::ecs::InventorySetup");
    if let Err(off) = &containers {
        enshwick_loader_api::warn!(
            "Safes nobody looked at this session lend their stock to crafting: {off}"
        );
    }
    let containers = containers.ok();
    load.scheduler()?.every(Duration::from_secs(2), move || {
        if let Some(containers) = containers {
            learn_safes(world, containers);
        }
        let safes = ACCESS.safes.load();
        let mut learned = false;
        for (&entity, &(owner, at)) in safes.iter() {
            if done.contains(&entity) {
                continue;
            }
            let named = world.has_label(Entity(entity)) == Some(true);
            if named {
                done.insert(entity);
                continue;
            }
            let mut name = ACCESS
                .owners
                .load()
                .get(&at)
                .map(|safe| safe.owner_name.clone())
                .unwrap_or_default();
            if name.is_empty()
                && let Some(online) = world.player_by_key(owner)
            {
                name = online.name;
                ACCESS.change(at, |safe| safe.owner_name.clone_from(&name));
                learned = true;
            }
            if name.is_empty() {
                continue;
            }
            let label = phrases.say("safe.label", &[("owner", &name)]);
            if world.set_label(Entity(entity), &label) {
                done.insert(entity);
            }
        }
        if learned {
            save(store);
        }
        done.retain(|entity| safes.contains_key(entity));
        std::ops::ControlFlow::Continue(())
    });
    Ok(())
}

/// How many server steps apart the players snapshot is retaken for a
/// character it lacks.
const CATCH_UP_STEPS: u64 = 4;

fn seen(player: &enshwick_loader_api::world::Player) -> Seen {
    Seen {
        key: player.key,
        admin: player.is_admin(),
    }
}

/// Retakes the players snapshot from the world, on the game thread.
fn snapshot_players(world: World) {
    let players: HashMap<u32, Seen> = world
        .players()
        .iter()
        .map(|p| (p.entity.0, seen(p)))
        .collect();
    ACCESS.players.store(std::sync::Arc::new(players));
}

/// Keeps the connected players' keys and administrator flags current: on
/// joining and leaving, every 10 s for permissions, and within a few steps
/// of a check that met a character the snapshot lacks.
fn track_players(load: &mut Load<'_>, world: World) -> Result<(), Off> {
    load.on::<PlayerJoined>(move |joined| {
        let entity = Entity(joined.entity);
        let player = world.player(entity).map_or(
            Seen {
                key: joined.key,
                admin: false,
            },
            |p| seen(&p),
        );
        ACCESS.players.rcu(|players| {
            let mut players = HashMap::clone(players);
            players.insert(entity.0, player);
            players
        });
    })?;
    load.on::<PlayerLeft>(|left| {
        ACCESS.players.rcu(|players| {
            let mut players = HashMap::clone(players);
            players.remove(&left.entity);
            players
        });
    })?;
    let caught_up = AtomicU64::new(0);
    load.on::<Ticked>(move |tick| {
        if !ACCESS.unregistered.load(Ordering::Relaxed)
            || tick.step.wrapping_sub(caught_up.load(Ordering::Relaxed)) < CATCH_UP_STEPS
        {
            return;
        }
        ACCESS.unregistered.store(false, Ordering::Relaxed);
        caught_up.store(tick.step, Ordering::Relaxed);
        snapshot_players(world);
    })?;
    // Permissions can change while a player is online.
    load.scheduler()?.every(Duration::from_secs(10), move || {
        snapshot_players(world);
        std::ops::ControlFlow::Continue(())
    });
    Ok(())
}

enshwick_loader_api::declare_mod!(Safes);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_parse() {
        use enshwick_loader_api::config::Config;
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(settings.cost_multiplier, 2);
        assert!(settings.admins_can_open);
        assert!(settings.validate().is_ok());
        assert_eq!(settings.label_language, "en");
        let zero = Settings {
            cost_multiplier: 0,
            ..Settings::default()
        };
        assert!(zero.validate().is_err());
        let unknown = Settings {
            label_language: "xx".into(),
            ..Settings::default()
        };
        assert!(unknown.validate().is_err());
    }

    #[test]
    fn a_world_s_cost_applies_when_it_loads() {
        use enshwick_loader_api::abi::Status;
        use enshwick_loader_api::events::WorldSettings;

        let mut host = enshwick_loader_api::test::FakeHost::new()
            .field_offset("keen::RecipeRegistryResource", "recipes", 8)
            .type_size("keen::RecipeInfo", 0xa8)
            .type_size("keen::RecipeInputResource", 48)
            .type_size("keen::RecipeItemStackResource", 24)
            .field_offset("keen::RecipeItemStackResource", "item", 0x10)
            .field_offset("keen::RecipeItemStackResource", "count", 0x14);
        for (field, at) in [
            ("recipeGuid", 0),
            ("recipeId", 0x10),
            ("workshopGuid", 0x14),
            ("workshopId", 0x24),
            ("input", 0x28),
            ("output", 0x30),
            ("requiredProps", 0x40),
            ("completionRequirementQuery", 0x5c),
            ("debugName", 0x78),
        ] {
            host = host.field_offset("keen::RecipeInfo", field, at);
        }
        for (field, at) in [
            ("itemId", 0),
            ("objectId", 4),
            ("name", 0x48),
            ("iconImage", 0x58),
            ("description", 0x68),
            ("flags", 0x78),
            ("pidEntity", 0x80),
            ("uiValues", 0x90),
        ] {
            host = host.field_offset("keen::ItemInfo", field, at);
        }
        for (field, at) in [
            ("locaId", 0),
            ("valueFormat", 4),
            ("value", 8),
            ("isSigned", 0x24),
        ] {
            host = host.field_offset("keen::ImpactUiValue", field, at);
        }
        let loader = host
            .type_size("keen::ImpactUiValue", 0x28)
            .type_hash("keen::ecs::CurrentTransform", 1)
            .type_hash("keen::ecs::UsedItem", 2)
            .field_offset("keen::ItemRegistryResource", "itemRefs", 0)
            .field_offset("keen::ItemRegistryResource", "dbgNames", 0x38)
            .settings(
                "cost_multiplier = 2
label_language = \"en\"
",
            )
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let allowed = loader.rebuilds().len();
        assert_eq!(allowed, 1, "the registry may be rebuilt");
        loader.set_settings(
            "cost_multiplier = 2
label_language = \"en\"
",
        );
        assert!(loader.raise::<WorldSettings>(&()) >= 1);
        assert_eq!(
            loader.rebuilds().len(),
            allowed,
            "the same cost: no rebuild"
        );
        loader.set_settings(
            "cost_multiplier = 5
label_language = \"uk\"
",
        );
        assert!(loader.raise::<WorldSettings>(&()) >= 1);
        assert_eq!(COST.load(Ordering::Relaxed), 5);
        assert_eq!(
            loader.rebuilds().len(),
            allowed + 1,
            "the world's cost: rebuilt"
        );
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn settings_from_0_1_still_parse() {
        use enshwick_loader_api::config::Config;
        let old: Settings =
            toml::from_str("cost_multiplier = 3\nadmins_can_open = false\n").unwrap();
        assert_eq!(old.cost_multiplier, 3);
        assert_eq!(old.label_language, "en");
        assert!(old.validate().is_ok());
    }

    #[test]
    fn owners_survive_the_file_with_keys_past_i64() {
        let mut friendly = Safe::new(5, "Ann");
        friendly.allow(u64::MAX - 1, "Bob Lee");
        let owners = HashMap::from([((1, -2, 3), Safe::new(u64::MAX, "")), ((0, 0, 0), friendly)]);
        let text = toml::to_string(&Owners::from_map(&owners)).unwrap();
        let back: Owners = toml::from_str(&text).unwrap();
        assert_eq!(back.to_map(), owners);
    }

    #[test]
    fn owners_files_from_0_1_still_load() {
        let old = "[[safes]]\ncell = [1, 2, 3]\nowner = \"00000000000000ff\"\n";
        let back: Owners = toml::from_str(old).unwrap();
        assert_eq!(
            back.to_map(),
            HashMap::from([((1, 2, 3), Safe::new(0xff, ""))])
        );
    }
}
