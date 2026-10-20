//! Tools for test servers: `/dev` commands for administrators, and a command
//! file whose lines run as the server console's commands.
//!
//! Off until `enabled = true` in `mod-configs/dev-tools.toml`. While on, an
//! administrator types `/dev world`, `/dev mods` and the like in the chat,
//! and every line appended to `mod-data/dev-tools/commands.txt` runs on the
//! server's game thread as if the console had typed it, any mod's command
//! included; each line and its reply go to `logs/enshwick/mods/dev-tools.log`.
//! The commands use only what the platform offers: world reads, chat, the
//! inventory API and settings reloads; time and teleports wait until the
//! platform has them.

pub mod command;
pub mod file;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use enshwick_api::commands::{CommandContext, Reply};
use enshwick_api::ecs::Entity;
use enshwick_api::events::BuildingPlaced;
use enshwick_api::inventory::Inventories;
use enshwick_api::schedule::Scheduler;
use enshwick_api::world::World;
use enshwick_api::{Host, Load, Mod, Off};

use crate::command::{Dev, Online, Placed, Seen};
use crate::file::Tail;

/// How often the command file is looked at; one stat call each time.
const POLL: Duration = Duration::from_secs(1);
/// The command file, in the mod's data folder.
const COMMAND_FILE: &str = "commands.txt";
/// Placed pieces `/dev placed` remembers.
const REMEMBERED: usize = 32;

/// Dev tools: commands and a command file for test servers. Keep them off
/// on a server people play on.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, enshwick_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Turn on the /dev commands for administrators and the command file
    /// mod-data/dev-tools/commands.txt, whose new lines run as the server
    /// console's commands.
    enabled: bool,
}

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Command file lines run since start.
static RUN: AtomicU64 = AtomicU64::new(0);
/// The last pieces placed, newest last.
static PLACED: Mutex<VecDeque<Placed>> = Mutex::new(VecDeque::new());

fn set_enabled(settings: &Settings) {
    let was = ENABLED.swap(settings.enabled, Ordering::AcqRel);
    if was != settings.enabled {
        enshwick_api::info!("dev tools {}", if settings.enabled { "on" } else { "off" });
    }
}

/// The dev tools.
#[derive(Debug)]
pub struct DevTools {
    file: PathBuf,
    scheduler: Scheduler,
}

impl Mod for DevTools {
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(
            id: "dev-tools",
            side: SERVER,
            updates: github("themaiby/enshwick", "enshwick-dev-tools"),
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let settings: Settings = load.config()?;
        set_enabled(&settings);
        load.on_config_change(|settings: Settings| set_enabled(&settings))?;
        let data = load.data_dir()?;
        let game = data
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| Off::new("the data folder is not under the game's folder"))?
            .to_path_buf();
        let world = load.world()?;
        let inventories = load.inventories().ok();
        load.on::<BuildingPlaced>(remember)?;
        let tools = Tools {
            world,
            inventories,
            game,
        };
        load.command(
            "dev",
            "test-server tools for administrators; /dev help",
            move |ctx, args| dev(&tools, ctx, args),
        )?;
        load.status(|| {
            let state = if ENABLED.load(Ordering::Acquire) {
                "on"
            } else {
                "off"
            };
            format!(
                "{state}; {} command file lines run",
                RUN.load(Ordering::Relaxed)
            )
        })?;
        Ok(Self {
            file: data.join(COMMAND_FILE),
            scheduler: load.scheduler()?,
        })
    }

    fn ready(&'static self, _host: Host) {
        let started = std::thread::Builder::new()
            .name("enshwick-dev-tools".into())
            .spawn(move || watch(&self.file, self.scheduler));
        if let Err(e) = started {
            enshwick_api::warn!("the command file is not watched: {e}");
        }
    }
}

enshwick_api::declare_mod!(DevTools);

/// Remembers a placed piece for `/dev placed`.
fn remember(data: &enshwick_api::abi::BuildingData) {
    let mut placed = PLACED.lock().unwrap_or_else(PoisonError::into_inner);
    if placed.len() == REMEMBERED {
        placed.pop_front();
    }
    placed.push_back(Placed {
        entity: data.entity,
        item: data.item,
        template: data.template,
        position: data.position,
    });
}

/// What the commands work with.
#[derive(Debug)]
struct Tools {
    world: World,
    inventories: Option<Inventories>,
    game: PathBuf,
}

/// `/dev`, for administrators while the tools are on.
fn dev(tools: &Tools, ctx: &CommandContext, args: &[&str]) -> Reply {
    let world = &tools.world;
    if !ctx.is_admin() {
        return Reply::from("Only administrators use /dev.");
    }
    if !ENABLED.load(Ordering::Acquire) {
        return Reply::from("Dev tools are off: set enabled = true in mod-configs/dev-tools.toml.");
    }
    let dev = match command::parse(args) {
        Ok(dev) => dev,
        Err(why) => return Reply::from(why),
    };
    Reply::from(match dev {
        Dev::Help => command::HELP.to_owned(),
        Dev::World => command::world(&online(world)),
        Dev::Entity(id) => command::entity(&seen(world, id)),
        Dev::Placed(count) => {
            let placed = PLACED.lock().unwrap_or_else(PoisonError::into_inner);
            let newest: Vec<Placed> = placed.iter().rev().take(count).copied().collect();
            command::placed(&newest)
        }
        Dev::Mods => command::mods(&enshwick_api::mods()),
        Dev::Reload(id) => command::reload(&tools.game, &id),
        Dev::Give { item, count, to } => give(tools, ctx, item, count, to.as_deref()),
        Dev::Say(text) => {
            world.broadcast(&text);
            format!("said: {text}")
        }
    })
}

/// `/dev give`: queues the items for the named or typing player.
fn give(tools: &Tools, ctx: &CommandContext, item: u32, count: u32, to: Option<&str>) -> String {
    let Some(inventories) = tools.inventories else {
        return "giving items needs the inventory API, which this game build lacks".to_owned();
    };
    let player = match to {
        Some(name) => tools
            .world
            .players()
            .into_iter()
            .find(|p| p.name.eq_ignore_ascii_case(name)),
        None if ctx.player.entity.0 != 0 => Some(ctx.player.clone()),
        None => {
            return "the console names the player: /dev give <item> <count> <player>".to_owned();
        }
    };
    let Some(player) = player else {
        return format!("no player {} online", to.unwrap_or_default());
    };
    let world = tools.world;
    let receiver = player.clone();
    inventories.give(player.entity, item, count, move |outcome| {
        let line = if outcome.is_committed() {
            format!("gave {count} of item {item} to {}", receiver.name)
        } else {
            format!(
                "giving {count} of item {item} to {} failed: {:?}",
                receiver.name, outcome.done
            )
        };
        enshwick_api::info!("{line}");
        world.tell(&receiver, &line);
    });
    format!("giving {count} of item {item} to {}", player.name)
}

fn online(world: &World) -> Vec<Online> {
    world
        .players()
        .into_iter()
        .map(|p| Online {
            admin: p.is_admin(),
            position: world.position(p.entity),
            entity: p.entity.0,
            slot: p.slot,
            name: p.name,
        })
        .collect()
}

fn seen(world: &World, id: u32) -> Seen {
    let entity = Entity(id);
    Seen {
        entity: id,
        template: world.template(entity),
        item: world.used_item(entity),
        position: world.position(entity),
        player: world.player(entity).map(|p| p.name),
    }
}

/// What the command file looked like when last seen: modified time and length.
type Stamp = Option<(SystemTime, u64)>;

fn stamp(path: &Path) -> Stamp {
    let meta = std::fs::metadata(path).ok()?;
    Some((meta.modified().ok()?, meta.len()))
}

/// Watches the command file for good, handing new lines to the game thread
/// while the tools are on.
fn watch(path: &Path, scheduler: Scheduler) {
    let mut seen = stamp(path);
    let mut tail = Tail::at(&std::fs::read(path).unwrap_or_default());
    enshwick_api::info!("watching {}", path.display());
    loop {
        std::thread::sleep(POLL);
        let now = stamp(path);
        if now == seen {
            continue;
        }
        seen = now;
        let lines = tail.feed(&std::fs::read(path).unwrap_or_default());
        if lines.is_empty() {
            continue;
        }
        if !ENABLED.load(Ordering::Acquire) {
            enshwick_api::info!(
                "{} command file lines skipped: dev tools are off",
                lines.len()
            );
            continue;
        }
        scheduler.next_tick(move || {
            for line in lines {
                run(&line);
            }
        });
    }
}

/// Runs a command file line as the console and logs its reply.
fn run(line: &str) {
    RUN.fetch_add(1, Ordering::Relaxed);
    match enshwick_api::commands::run(line) {
        Ok(reply) if reply.text().is_empty() => enshwick_api::info!("{line}: no reply"),
        Ok(reply) => enshwick_api::info!("{line}:\n{}", reply.text()),
        Err(why) => enshwick_api::warn!("{line}: {why}"),
    }
}

#[cfg(test)]
mod tests {
    use enshwick_api::abi::{Header, PlayerInfo, Status};

    use super::*;

    fn player(permissions: u8) -> PlayerInfo {
        PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: 9,
            key: 7,
            permissions,
            name_len: 3,
            name: {
                let mut name = [0; 64];
                name[..3].copy_from_slice(b"Ann");
                name
            },
        }
    }

    #[test]
    fn dev_answers_administrators_while_on_and_runs_file_lines_as_the_console() {
        let loader = enshwick_test::FakeHost::new()
            .type_hash("keen::ecs::CurrentTransform", 1)
            .type_hash("keen::ecs::UsedItem", 2)
            .foreign_command("enshwick", "Enshwick 0.0.0-test: dev-tools 0.1.0")
            .settings("enabled = false\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.commands()[0].0, "dev");

        let admin = player(PlayerInfo::CAN_KICK_BAN);
        let off = loader.command("dev", &admin, "world").unwrap();
        assert!(off.starts_with("Dev tools are off"));
        assert_eq!(
            loader.command("dev", &player(0), "world").as_deref(),
            Some("Only administrators use /dev.")
        );

        loader.set_settings("enabled = true\n");
        assert_eq!(
            loader.raise::<enshwick_api::events::SettingsChanged>(&()),
            1
        );
        assert_eq!(
            loader.command("dev", &admin, "world").as_deref(),
            Some("no players online")
        );
        assert!(
            loader
                .command("dev", &admin, "help")
                .unwrap()
                .contains("/dev mods")
        );
        assert!(loader.status()[0].starts_with("on; "));

        run("/enshwick status");
        run("/dev world");
        assert_eq!(loader.console(), ["/enshwick status", "/dev world"]);
        let logs = loader.logs();
        assert!(logs.iter().any(|(_, l)| l.contains("Enshwick 0.0.0-test")));
        assert!(logs.iter().any(|(_, l)| l.contains("no players online")));
        assert!(loader.faults().is_empty());
    }
}
