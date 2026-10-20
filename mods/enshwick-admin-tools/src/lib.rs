//! Tools for test servers: `/admin` commands for administrators, and a command
//! file whose lines run as the server console's commands.
//!
//! Off until `enabled = true` in `mod-configs/admin-tools.toml`. While on, an
//! administrator types `/admin world`, `/admin mods` and the like in the chat,
//! and every line appended to `mod-data/admin-tools/commands.txt` runs on the
//! server's game thread as if the console had typed it, any mod's command
//! included; each line and its reply go to `logs/enshwick/mods/admin-tools.log`.
//! The commands use only what the platform offers: world reads, chat, the
//! inventory API and settings reloads; time and teleports wait until the
//! platform has them.
//!
//! The mod was `dev-tools` before: its settings file and data folder move to
//! the new id on the first start, and `/dev` still answers as `/admin` for
//! one release.

pub mod command;
pub mod file;
mod migrate;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use enshwick_loader_api::commands::{CommandContext, Reply};
use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::events::BuildingPlaced;
use enshwick_loader_api::inventory::Inventories;
use enshwick_loader_api::schedule::Scheduler;
use enshwick_loader_api::world::World;
use enshwick_loader_api::{Host, Load, Mod, Off};

use crate::command::{Admin, Online, Placed, Seen};
use crate::file::Tail;

/// How often the command file is looked at; one stat call each time.
const POLL: Duration = Duration::from_secs(1);
/// The command file, in the mod's data folder.
const COMMAND_FILE: &str = "commands.txt";
/// Placed pieces `/admin placed` remembers.
const REMEMBERED: usize = 32;
/// The mod's id, as `manifest!` spells it.
const ID: &str = "admin-tools";

/// Admin tools: commands and a command file for test servers. Keep them off
/// on a server people play on.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Turn on the /admin commands for administrators and the command file
    /// mod-data/admin-tools/commands.txt, whose new lines run as the server
    /// console's commands.
    #[config(runtime)]
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
        enshwick_loader_api::info!(
            "admin tools {}",
            if settings.enabled { "on" } else { "off" }
        );
    }
}

/// The admin tools.
#[derive(Debug)]
pub struct AdminTools {
    file: PathBuf,
    scheduler: Scheduler,
}

impl Mod for AdminTools {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(
            id: "admin-tools",
            side: SERVER,
            updates: github("themaiby/enshwick", "enshwick-admin-tools"),
            signed_by: ["RWTjqgtfpy3lSOLC4uaUXuvvKSLQKOFf6nUnEytoiFjSzxMEhLnoAdJF"],
        )
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let data = load.data_dir()?;
        let game = data
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| Off::new("the data folder is not under the game's folder"))?
            .to_path_buf();
        for line in migrate::migrate(&game, ID) {
            enshwick_loader_api::info!("{line}");
        }
        let settings: Settings = load.config()?;
        set_enabled(&settings);
        load.on_config_change(|settings: Settings| set_enabled(&settings))?;
        let world = load.world()?;
        let inventories = load.inventories().ok();
        load.on::<BuildingPlaced>(remember)?;
        let tools = Arc::new(Tools {
            world,
            inventories,
            game,
        });
        let old = Arc::clone(&tools);
        load.add_command(
            "admin",
            "test-server tools for administrators; /admin help",
            move |ctx, args| admin(&tools, ctx, args),
        )?;
        load.add_command(
            "dev",
            "the old name of /admin, gone in the next release",
            move |ctx, args| renamed(&admin(&old, ctx, args)),
        )?;
        load.add_status(|| {
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
            .name("enshwick-admin-tools".into())
            .spawn(move || watch(&self.file, self.scheduler));
        if let Err(e) = started {
            enshwick_loader_api::warn!("the command file is not watched: {e}");
        }
    }
}

enshwick_loader_api::declare_mod!(AdminTools);

/// Remembers a placed piece for `/admin placed`.
fn remember(data: &enshwick_loader_api::abi::BuildingData) {
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

/// `/admin`, for administrators while the tools are on.
fn admin(tools: &Tools, ctx: &CommandContext, args: &[&str]) -> Reply {
    let world = &tools.world;
    if !ctx.is_admin() {
        return Reply::from("Only administrators use /admin.");
    }
    if !ENABLED.load(Ordering::Acquire) {
        return Reply::from(
            "Admin tools are off: set enabled = true in mod-configs/admin-tools.toml.",
        );
    }
    let admin = match command::parse(args) {
        Ok(admin) => admin,
        Err(why) => return Reply::from(why),
    };
    Reply::from(match admin {
        Admin::Help => command::HELP.to_owned(),
        Admin::World => command::world(&online(world)),
        Admin::Entity(id) => command::entity(&seen(world, id)),
        Admin::Placed(count) => {
            let placed = PLACED.lock().unwrap_or_else(PoisonError::into_inner);
            let newest: Vec<Placed> = placed.iter().rev().take(count).copied().collect();
            command::placed(&newest)
        }
        Admin::Mods => command::mods(&enshwick_loader_api::mods()),
        Admin::Reload(id) => command::reload(&tools.game, &id),
        Admin::Give { item, count, to } => give(tools, ctx, item, count, to.as_deref()),
        Admin::Say(text) => {
            world.broadcast(&text);
            format!("said: {text}")
        }
    })
}

/// `/dev`'s reply: `/admin`'s, with a note that the name changed.
fn renamed(reply: &Reply) -> Reply {
    Reply::from(format!(
        "{}\n(/dev is now /admin; /dev goes away in the next release)",
        reply.text()
    ))
}

/// `/admin give`: queues the items for the named or typing player.
fn give(tools: &Tools, ctx: &CommandContext, item: u32, count: u32, to: Option<&str>) -> String {
    let Some(inventories) = tools.inventories else {
        return "giving items needs the inventory API, which this game build lacks".to_owned();
    };
    let player = match to {
        Some(name) => tools.world.players_named(name).into_iter().next(),
        None if ctx.player.entity.0 != 0 => Some(ctx.player.clone()),
        None => {
            return "the console names the player: /admin give <item> <count> <player>".to_owned();
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
        enshwick_loader_api::info!("{line}");
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
    enshwick_loader_api::info!("watching {}", path.display());
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
            enshwick_loader_api::info!(
                "{} command file lines skipped: admin tools are off",
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
    match enshwick_loader_api::commands::run(line) {
        Ok(reply) if reply.text().is_empty() => enshwick_loader_api::info!("{line}: no reply"),
        Ok(reply) => enshwick_loader_api::info!("{line}:\n{}", reply.text()),
        Err(why) => enshwick_loader_api::warn!("{line}: {why}"),
    }
}

#[cfg(test)]
mod tests {
    use enshwick_loader_api::abi::{Header, PlayerInfo, Status};

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
        let loader = enshwick_loader_api::test::FakeHost::new()
            .type_hash("keen::ecs::CurrentTransform", 1)
            .type_hash("keen::ecs::UsedItem", 2)
            .foreign_command("enshwick", "Enshwick 0.0.0-test: admin-tools 0.1.0")
            .settings("enabled = false\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.commands()[0].0, "admin");
        assert_eq!(loader.commands()[1].0, "dev");

        let admin = player(PlayerInfo::CAN_KICK_BAN);
        let off = loader.command("admin", &admin, "world").unwrap();
        assert!(off.starts_with("Admin tools are off"));
        assert_eq!(
            loader.command("admin", &player(0), "world").as_deref(),
            Some("Only administrators use /admin.")
        );

        loader.set_settings("enabled = true\n");
        assert_eq!(
            loader.raise::<enshwick_loader_api::events::SettingsChanged>(&()),
            1
        );
        assert_eq!(
            loader.command("admin", &admin, "world").as_deref(),
            Some("no players online")
        );
        let old = loader.command("dev", &admin, "world").unwrap();
        assert!(old.starts_with("no players online\n(/dev is now /admin"));
        assert!(
            loader
                .command("admin", &admin, "help")
                .unwrap()
                .contains("/admin mods")
        );
        assert!(loader.status()[0].starts_with("on; "));

        run("/enshwick status");
        run("/admin world");
        assert_eq!(loader.console(), ["/enshwick status", "/admin world"]);
        let logs = loader.logs();
        assert!(logs.iter().any(|(_, l)| l.contains("Enshwick 0.0.0-test")));
        assert!(logs.iter().any(|(_, l)| l.contains("no players online")));
        assert!(loader.faults().is_empty());
    }
}
