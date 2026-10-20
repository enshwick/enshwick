//! The Safe: a chest crafted beside the wooden chest that only the player who
//! placed it, and the players they allow with `/safe allow`, can open or
//! empty; only the owner picks it up, and nobody upgrades it away. The owner
//! shows on hover, as the chest's name.
//!
//! The Safe item places the vanilla wooden chest, so a Safe without the mod is
//! an ordinary chest with every item still in it. The placed chest keeps the
//! Safe's item id in the save, which tells the mod which chests are Safes;
//! owners and the players they allowed are kept in
//! `mod-data/safes/owners.toml` by position.
//!
//! The refusals run on the dedicated server and, on a client, in the built-in
//! server while it hosts a game; the client's own world is left alone.

pub mod access;
pub mod command;
pub mod content;
mod hooks;

use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::Duration;

use enshwick_loader_api::abi::BuildingData;
use enshwick_loader_api::abi::Side;
use enshwick_loader_api::commands::Rename;
use enshwick_loader_api::ecs::{Components, Ecs, Entity, Worlds};
use enshwick_loader_api::events::{
    BuildingPlaced, BuildingRemoved, PlayerJoined, PlayerLeft, Shutdown, WorldSaved,
};
use enshwick_loader_api::i18n::Language;
use enshwick_loader_api::state::Store;
use enshwick_loader_api::world::{ComponentType, World};
use enshwick_loader_api::{Load, Mod, Off};

use crate::access::{ACCESS, Action, Allowed, Cell, Reads, Safe, Seen, permits};
use crate::command::Phrases;

/// Safe settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(default, deny_unknown_fields)]
#[config(validate = Settings::check)]
struct Settings {
    /// The Safe costs the wooden chest's ingredients times this.
    cost_multiplier: u32,
    /// Whether the server's administrators may open, remove and rename every Safe.
    admins_can_open: bool,
    /// The language of the owner's name on a Safe and of `/safe` replies: "en", "uk", "de", ...
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
        if !(1..=100).contains(&self.cost_multiplier) {
            return Err("cost_multiplier is 1 to 100".into());
        }
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

    fn into_map(self) -> HashMap<Cell, Safe> {
        self.safes
            .into_iter()
            .filter_map(|r| {
                let safe = Safe {
                    owner: u64::from_str_radix(&r.owner, 16).ok()?,
                    owner_name: r.name,
                    allowed: r
                        .allowed
                        .into_iter()
                        .filter_map(|a| {
                            Some(Allowed {
                                key: u64::from_str_radix(&a.key, 16).ok()?,
                                name: a.name,
                            })
                        })
                        .collect(),
                };
                Some(((r.cell[0], r.cell[1], r.cell[2]), safe))
            })
            .collect()
    }
}

/// Writes the owners on a thread of their own, one write at a time.
fn save(store: &'static Store<Owners>) {
    static WRITING: Mutex<()> = Mutex::new(());
    let spawned = std::thread::Builder::new()
        .name("safes-save".into())
        .spawn(move || {
            let _one = WRITING
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            // Read under the lock: the last write is always the newest map,
            // never an older copy whose thread happened to run last.
            let owners = Owners::from_map(&ACCESS.owners.load());
            if let Err(off) = store.save(&owners) {
                enshwick_loader_api::warn!("owners not saved: {off}");
            }
        });
    if spawned.is_err() {
        enshwick_loader_api::warn!("owners not saved: no thread");
    }
}

/// The mod's state.
#[derive(Debug)]
pub struct Safes;

impl Mod for Safes {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "safes",
            side: BOTH,
            join = true,
            updates: github("themaiby/enshwick", "enshwick-safes"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
            builds: [
                {
                    side: SERVER,
                    guid: "1c129660cd6efc4f9f0b21652d5ff2bf",
                    age: 387,
                    symbols: [
                        "safes.offer_test" => 0x9_e270,
                        "safes.transfer" => 0x19_81f0,
                        "safes.sort" => 0x19_8cf0,
                        "safes.base_stock" => 0x20_1800,
                        "safes.dismantle_method" => 0x9_67f0,
                        "safes.inventory_actions" => 0x19_f540,
                    ],
                    constants: [
                        "safes.state_input" => 0x10,
                        "safes.state_owner_index" => 0x178,
                        "safes.transfer_source" => 0x0c,
                        "safes.transfer_target" => 0x10,
                        "safes.transfer_source_slot" => 0x14,
                        "safes.transfer_target_slot" => 0x1c,
                        "safes.sort_target" => 0x74,
                        "safes.object_id" => 0x10,
                        "safes.object_kind" => 0x18,
                        "safes.deposit_target" => 0x100,
                        "safes.deposit_version" => 0x108,
                        "safes.consumed_deposit" => 0x48,
                        "safes.upgrade_target" => 0x10c,
                        "safes.upgrade_version" => 0x144,
                        "safes.consumed_upgrade" => 0x4c,
                    ],
                },
                {
                    // The built-in server's twins. The client's cursor context
                    // is 0x10 larger, but the dismantle hook reads only the
                    // object and the context's first word, which hold.
                    side: CLIENT,
                    guid: "69f0e9573b32c24a8838d3fff550824c",
                    age: 410,
                    symbols: [
                        "safes.offer_test" => 0x2a_0850,
                        "safes.transfer" => 0x3f_62e0,
                        "safes.sort" => 0x3f_6de0,
                        "safes.base_stock" => 0x46_82e0,
                        "safes.base_stock_ui_return" => 0x2b_884b,
                        "safes.dismantle_method" => 0x28_a170,
                        "safes.inventory_actions" => 0x3f_d610,
                    ],
                    constants: [
                        "safes.state_input" => 0x10,
                        "safes.state_owner_index" => 0x178,
                        "safes.transfer_source" => 0x0c,
                        "safes.transfer_target" => 0x10,
                        "safes.transfer_source_slot" => 0x14,
                        "safes.transfer_target_slot" => 0x1c,
                        "safes.sort_target" => 0x74,
                        "safes.object_id" => 0x10,
                        "safes.object_kind" => 0x18,
                        "safes.deposit_target" => 0x100,
                        "safes.deposit_version" => 0x108,
                        "safes.consumed_deposit" => 0x48,
                        "safes.upgrade_target" => 0x10c,
                        "safes.upgrade_version" => 0x144,
                        "safes.consumed_upgrade" => 0x4c,
                    ],
                },
            ],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        let cost = settings.cost_multiplier;
        let texts = load.texts(content::TEXTS)?;
        load.add_texts(
            &texts,
            &[
                ("safe.name", content::SAFE_NAME),
                ("safe.description", content::SAFE_DESCRIPTION),
            ],
        )?;
        load.add_item(content::item())?;
        load.add_recipe(content::recipe(cost))?;
        ACCESS
            .admins_open
            .store(settings.admins_can_open, Ordering::Relaxed);
        let phrases: &'static Phrases = Box::leak(Box::new(Phrases {
            texts,
            language: settings.language().unwrap_or(Language::FALLBACK),
        }));
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
    let reads = Reads {
        ecs: Ecs::resolve(load)?,
        components: Components::resolve(load)?,
        worlds: Worlds::resolve(load)?,
        used_item: world.component_type(load, "keen::ecs::UsedItem")?.hash(),
        transform: world
            .component_type(load, "keen::ecs::CurrentTransform")?
            .hash(),
        safe_item: content::SAFE.id(),
    };
    let constant = |name: &str| {
        load.constant(name)
            .map(|v| usize::try_from(v).unwrap_or(usize::MAX))
    };
    let layout = hooks::Layout {
        state_input: constant("safes.state_input")?,
        state_owner_index: constant("safes.state_owner_index")?,
        transfer_source: constant("safes.transfer_source")?,
        transfer_target: constant("safes.transfer_target")?,
        transfer_source_slot: constant("safes.transfer_source_slot")?,
        transfer_target_slot: constant("safes.transfer_target_slot")?,
        sort_target: constant("safes.sort_target")?,
        object_id: constant("safes.object_id")?,
        object_kind: constant("safes.object_kind")?,
        deposit_target: constant("safes.deposit_target")?,
        deposit_version: constant("safes.deposit_version")?,
        consumed_deposit: constant("safes.consumed_deposit")?,
        upgrade_target: constant("safes.upgrade_target")?,
        upgrade_version: constant("safes.upgrade_version")?,
        consumed_upgrade: constant("safes.consumed_upgrade")?,
        // The client's own interface asks for base stock directly from here.
        stock_ui_return: if client {
            load.symbol("safes.base_stock_ui_return")?
        } else {
            0
        },
        player_input: world.component_type(load, "keen::ecs::PlayerInput")?.hash(),
        consumed_input: world
            .component_type(load, "keen::ecs::ServerConsumedPlayerInput")?
            .hash(),
    };

    let store: &'static Store<Owners> =
        Box::leak(Box::new(Store::new(&load.data_dir()?, "owners")));
    let owners = store.load()?.into_map();
    enshwick_loader_api::info!("{} Safe owners loaded", owners.len());
    ACCESS.load_owners(owners);

    hooks::link(load, reads, layout)?;
    track_players(load, world)?;
    load.on::<BuildingPlaced>(move |placed| {
        if placed.item == content::SAFE.id() {
            placed_safe(world, phrases, placed);
            save(store);
        }
    })?;
    load.on::<BuildingRemoved>(move |removed| {
        if ACCESS.removed(removed.entity) {
            enshwick_loader_api::info!("Safe {} removed", removed.entity);
            save(store);
        }
    })?;
    load.on::<WorldSaved>(move |()| save(store))?;
    if client {
        // Entities and players belong to one hosted game; owners stay.
        load.on::<Shutdown>(|()| ACCESS.forget_session())?;
        enshwick_loader_api::info!("the Safe's checks act while this client hosts a game");
    }
    load.on_config_change(|settings: Settings| {
        ACCESS
            .admins_open
            .store(settings.admins_can_open, Ordering::Relaxed);
    })?;
    label_seen(load, world, phrases, store)?;
    load.on_rename(move |rename| may_rename(world, rename))?;
    load.command(
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
        if world.used_item(rename.entity)? != content::SAFE.id() {
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

/// Names the owner on every Safe seen this session that has no name yet,
/// on the game thread; the owner's name comes from the record, or from the
/// owner when they are online.
fn label_seen(
    load: &mut Load<'_>,
    world: World,
    phrases: &'static Phrases,
    store: &'static Store<Owners>,
) -> Result<(), Off> {
    let label_type: ComponentType = world.component_type(load, "keen::ecs::CustomPlayerString")?;
    let mut done: HashSet<u32> = HashSet::new();
    load.scheduler()?.every(Duration::from_secs(2), move || {
        let safes = ACCESS.safes.load();
        let mut learned = false;
        for (&entity, &(owner, at)) in safes.iter() {
            if done.contains(&entity) {
                continue;
            }
            let named = world
                .component(Entity(entity), label_type)
                // SAFETY: a live CustomPlayerString is its string id.
                .is_some_and(|id| unsafe { id.cast::<u64>().read_unaligned() } != 0);
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
                && let Some(online) = world.players().into_iter().find(|p| p.key == owner)
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

/// Keeps the connected players' keys and administrator flags current.
fn track_players(load: &mut Load<'_>, world: World) -> Result<(), Off> {
    let seen = |player: &enshwick_loader_api::world::Player| Seen {
        key: player.key,
        admin: player.is_admin(),
    };
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
    // Permissions can change while a player is online.
    load.scheduler()?.every(Duration::from_secs(10), move || {
        let players: HashMap<u32, Seen> = world
            .players()
            .iter()
            .map(|p| (p.entity.0, seen(p)))
            .collect();
        ACCESS.players.store(std::sync::Arc::new(players));
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
        assert_eq!(back.into_map(), owners);
    }

    #[test]
    fn owners_files_from_0_1_still_load() {
        let old = "[[safes]]\ncell = [1, 2, 3]\nowner = \"00000000000000ff\"\n";
        let back: Owners = toml::from_str(old).unwrap();
        assert_eq!(
            back.into_map(),
            HashMap::from([((1, 2, 3), Safe::new(0xff, ""))])
        );
    }
}
