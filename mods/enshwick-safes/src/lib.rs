//! The Safe: a chest crafted beside the wooden chest that only the player who
//! placed it can open, empty, pick up or upgrade away.
//!
//! The Safe item places the vanilla wooden chest, so a Safe without the mod is
//! an ordinary chest with every item still in it. The placed chest keeps the
//! Safe's item id in the save, which tells the mod which chests are Safes;
//! owners are kept in `mod-data/safes/owners.toml` by position.

pub mod access;
pub mod content;
mod hooks;

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::Ordering;
use std::time::Duration;

use enshwick_api::abi::Side;
use enshwick_api::ecs::{Components, Ecs, Entity};
use enshwick_api::events::{BuildingPlaced, BuildingRemoved, PlayerJoined, PlayerLeft, WorldSaved};
use enshwick_api::state::Store;
use enshwick_api::world::World;
use enshwick_api::{Load, Mod, Off};

use crate::access::{ACCESS, Cell, Reads, Seen};

/// Safe settings.
#[derive(Debug, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
#[serde(deny_unknown_fields)]
#[config(validate = Settings::check)]
struct Settings {
    /// The Safe costs the wooden chest's ingredients times this.
    cost_multiplier: u32,
    /// Whether the server's administrators may open every Safe.
    admins_can_open: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            cost_multiplier: 2,
            admins_can_open: true,
        }
    }
}

impl Settings {
    fn check(&self) -> Result<(), String> {
        (1..=100)
            .contains(&self.cost_multiplier)
            .then_some(())
            .ok_or_else(|| "cost_multiplier is 1 to 100".into())
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
}

impl Owners {
    fn from_map(owners: &HashMap<Cell, u64>) -> Self {
        let mut safes: Vec<Record> = owners
            .iter()
            .map(|(&(x, y, z), &owner)| Record {
                cell: [x, y, z],
                owner: format!("{owner:016x}"),
            })
            .collect();
        safes.sort_by_key(|r| r.cell);
        Self { safes }
    }

    fn into_map(self) -> HashMap<Cell, u64> {
        self.safes
            .into_iter()
            .filter_map(|r| {
                Some((
                    (r.cell[0], r.cell[1], r.cell[2]),
                    u64::from_str_radix(&r.owner, 16).ok()?,
                ))
            })
            .collect()
    }
}

/// Writes the owners on a thread of their own, one write at a time.
fn save(store: &'static Store<Owners>) {
    static WRITING: Mutex<()> = Mutex::new(());
    let owners = Owners::from_map(&ACCESS.owners.load());
    let spawned = std::thread::Builder::new()
        .name("safes-save".into())
        .spawn(move || {
            let _one = WRITING
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if let Err(off) = store.save(&owners) {
                enshwick_api::warn!("owners not saved: {off}");
            }
        });
    if spawned.is_err() {
        enshwick_api::warn!("owners not saved: no thread");
    }
}

/// The mod's state.
#[derive(Debug)]
pub struct Safes;

impl Mod for Safes {
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(
            id: "safes",
            side: BOTH,
            join = true,
            builds: [{
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
            }],
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
        if load.host().build().side == Side::SERVER {
            ACCESS
                .admins_open
                .store(settings.admins_can_open, Ordering::Relaxed);
            guard(load)?;
        }
        enshwick_api::info!(
            "the Safe costs the wooden chest x{cost}; administrators may open Safes: {}",
            settings.admins_can_open
        );
        Ok(Self)
    }
}

/// The server half: owners, players and the refusals.
fn guard(load: &mut Load<'_>) -> Result<(), Off> {
    let world = load.world()?;
    let reads = Reads {
        ecs: Ecs::resolve(load)?,
        components: Components::resolve(load)?,
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
        player_input: world.component_type(load, "keen::ecs::PlayerInput")?.hash(),
        consumed_input: world
            .component_type(load, "keen::ecs::ServerConsumedPlayerInput")?
            .hash(),
    };

    let store: &'static Store<Owners> =
        Box::leak(Box::new(Store::new(&load.data_dir()?, "owners")));
    let owners = store.load()?.into_map();
    enshwick_api::info!("{} Safe owners loaded", owners.len());
    ACCESS.load_owners(owners);

    hooks::link(load, reads, layout)?;
    track_players(load, world)?;
    load.on::<BuildingPlaced>(move |placed| {
        if placed.item != content::SAFE.id() {
            return;
        }
        let player = Entity(placed.player);
        let key = ACCESS
            .players
            .load()
            .get(&player.0)
            .map(|seen| seen.key)
            .or_else(|| world.player(player).map(|p| p.key));
        let Some(key) = key else {
            enshwick_api::warn!("a Safe was placed by no known player and stays open to all");
            return;
        };
        let position = placed.position.map(f64::from);
        ACCESS.placed(placed.entity, access::cell(position), key);
        enshwick_api::info!(
            "Safe {} placed at {position:?} by {key:016x}",
            placed.entity
        );
        save(store);
    })?;
    load.on::<BuildingRemoved>(move |removed| {
        if ACCESS.removed(removed.entity) {
            enshwick_api::info!("Safe {} removed", removed.entity);
            save(store);
        }
    })?;
    load.on::<WorldSaved>(move |()| save(store))?;
    load.on_config_change(|settings: Settings| {
        ACCESS
            .admins_open
            .store(settings.admins_can_open, Ordering::Relaxed);
    })?;
    Ok(())
}

/// Keeps the connected players' keys and administrator flags current.
fn track_players(load: &mut Load<'_>, world: World) -> Result<(), Off> {
    let seen = |player: &enshwick_api::world::Player| Seen {
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

enshwick_api::declare_mod!(Safes);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_settings_parse() {
        use enshwick_api::config::Config;
        let settings: Settings = toml::from_str(&Settings::template()).unwrap();
        assert_eq!(settings.cost_multiplier, 2);
        assert!(settings.admins_can_open);
        assert!(settings.validate().is_ok());
        let zero = Settings {
            cost_multiplier: 0,
            admins_can_open: true,
        };
        assert!(zero.validate().is_err());
    }

    #[test]
    fn owners_survive_the_file_with_keys_past_i64() {
        let owners = HashMap::from([((1, -2, 3), u64::MAX), ((0, 0, 0), 5)]);
        let text = toml::to_string(&Owners::from_map(&owners)).unwrap();
        let back: Owners = toml::from_str(&text).unwrap();
        assert_eq!(back.into_map(), owners);
    }
}
