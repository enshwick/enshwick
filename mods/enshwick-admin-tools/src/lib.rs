//! Tools for test servers: administrator actions on the Mods screen, and a
//! command file whose lines run on the server's console.
//!
//! Off until `enabled = true` in the world's settings. While on, an
//! administrator finds `world`, `mods` and the other actions on the mod's
//! page of the Mods screen, and every line appended to
//! `mod-data/admin-tools/commands.txt` runs on the server's game thread as a
//! console line: `admin-tools world`, any mod's actions, or the loader's own
//! `enshwick ...`; each line and its answer go to
//! `logs/enshwick/mods/admin-tools.log`. The actions use only what the
//! platform offers: world reads, the inventory API and settings reloads.
//!
//! The mod was `dev-tools` before: its settings file and data folder move to
//! the new id on the first start.

pub mod answers;
pub mod file;
mod migrate;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use enshwick_loader_api::actions::{Action, Arg, Ask, Outcome, Subject, Value};
use enshwick_loader_api::ecs::Entity;
use enshwick_loader_api::events::BuildingPlaced;
use enshwick_loader_api::inventory::Inventories;
use enshwick_loader_api::schedule::Scheduler;
use enshwick_loader_api::world::World;
use enshwick_loader_api::{Host, Load, Mod, Off};

use crate::answers::{MOST, Online, Placed, Seen};
use crate::file::Tail;

/// How often the command file is looked at; one stat call each time.
const POLL: Duration = Duration::from_secs(1);
/// The command file, in the mod's data folder.
const COMMAND_FILE: &str = "commands.txt";
/// Placed pieces `placed` remembers.
const REMEMBERED: usize = 32;
/// The action labels, in English and Ukrainian.
const TEXTS: &str = r#"
[en]
"world" = "Players online"
"entity" = "An entity"
"placed" = "Pieces placed lately"
"mods" = "Loaded mods"
"reload" = "Reread a mod's settings"
"give" = "Give yourself items"
"give-to" = "Give a player items"
"arg.entity" = "Entity"
"arg.count" = "How many"
"arg.item" = "Item id"
"arg.mod" = "Mod"
"arg.player" = "Player"
"off" = "Admin tools are off: set enabled = true in their settings."
"no-character" = "The console has no character: use give-to."

[uk]
"world" = "Гравці в грі"
"entity" = "Сутність"
"placed" = "Нещодавно поставлене"
"mods" = "Завантажені моди"
"reload" = "Перечитати налаштування мода"
"give" = "Дати собі предмети"
"give-to" = "Дати гравцеві предмети"
"arg.entity" = "Сутність"
"arg.count" = "Скільки"
"arg.item" = "Id предмета"
"arg.mod" = "Мод"
"arg.player" = "Гравець"
"off" = "Інструменти адміністратора вимкнені: увімкніть enabled = true в їхніх налаштуваннях."
"no-character" = "У консолі немає персонажа: скористайтеся give-to."
"#;
/// The mod's id, as `manifest!` spells it.
const ID: &str = "admin-tools";

/// Admin tools: actions and a command file for test servers. Keep them off
/// on a server people play on.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, enshwick_loader_api::Config)]
#[serde(deny_unknown_fields)]
struct Settings {
    /// Turn on the administrator actions and the command file
    /// mod-data/admin-tools/commands.txt, whose new lines run on the server's
    /// console.
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
            name: "Admin tools",
            replaces: ["dev-tools"],
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
        // Entity ids belong to one game: a hosted game's next world starts
        // with nothing placed.
        load.on::<enshwick_loader_api::events::ShutdownStarted>(|()| {
            PLACED
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .clear();
        })?;
        let tools = Arc::new(Tools {
            world,
            inventories,
            game,
        });
        let texts = load.texts(TEXTS)?;
        declare(load, texts, &tools)?;
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

/// Remembers a placed piece for `placed`.
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

/// What the actions work with.
#[derive(Debug)]
struct Tools {
    world: World,
    inventories: Option<Inventories>,
    game: PathBuf,
}

/// Lets administrators run an action while the tools are on; why not,
/// otherwise.
fn while_on(ask: &Ask<'_>) -> Result<Subject, String> {
    if ENABLED.load(Ordering::Acquire) {
        Ok(Subject::new(0, ""))
    } else {
        Err(ask.say("off", &[]))
    }
}

/// An id from a number argument, which the platform kept within its range.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the argument's range is within u32"
)]
const fn id(value: f64) -> u32 {
    value as u32
}

/// Declares the administrator actions.
fn declare(
    load: &mut Load<'_>,
    texts: enshwick_loader_api::i18n::Texts,
    tools: &Arc<Tools>,
) -> Result<(), Off> {
    let actions = load.actions(texts)?;
    let any_id = 0.0..=f64::from(u32::MAX);
    let admin = |id: &str| Action::new(id, id).admins().at(while_on);
    let world = tools.world;
    actions.add(admin("world"), move |_| {
        Outcome::done(answers::world(&online(&world)))
    })?;
    actions.add(
        admin("entity").arg(Arg::number("arg.entity", any_id.clone(), 1.0)),
        move |request| match request.args {
            [Value::Number(entity)] => Outcome::done(answers::entity(&seen(&world, id(*entity)))),
            _ => Outcome::refused(""),
        },
    )?;
    #[expect(clippy::cast_precision_loss, reason = "a small count")]
    let most = REMEMBERED as f64;
    actions.add(
        admin("placed").arg(Arg::number("arg.count", 1.0..=most, 1.0)),
        |request| {
            let [Value::Number(count)] = request.args else {
                return Outcome::refused("");
            };
            let placed = PLACED.lock().unwrap_or_else(PoisonError::into_inner);
            let newest: Vec<Placed> = placed
                .iter()
                .rev()
                .take(id(*count) as usize)
                .copied()
                .collect();
            Outcome::done(answers::placed(&newest))
        },
    )?;
    actions.add(admin("mods"), |_| {
        Outcome::done(answers::mods(&enshwick_loader_api::mods()))
    })?;
    let ids: Vec<String> = enshwick_loader_api::mods()
        .into_iter()
        .map(|m| m.id)
        .filter(|id| id != enshwick_loader_api::mods::LOADER_ID)
        .collect();
    let choices: Vec<(&str, &str)> = ids.iter().map(|id| (id.as_str(), id.as_str())).collect();
    let reloading = Arc::clone(tools);
    let reload_ids = ids.clone();
    actions.add(
        admin("reload").arg(Arg::choice("arg.mod", &choices)),
        move |request| match request.args {
            [Value::Choice(index)] => {
                let id = &reload_ids[*index];
                let world = reloading.world.key();
                Outcome::done(answers::reload(&reloading.game, id, world.as_deref()))
            }
            _ => Outcome::refused(""),
        },
    )?;
    let count = Arg::number("arg.count", 1.0..=f64::from(MOST), 1.0);
    let item = Arg::number("arg.item", 1.0..=f64::from(u32::MAX), 1.0);
    let giving = Arc::clone(tools);
    actions.add(
        admin("give").arg(item.clone()).arg(count.clone()),
        move |request| match request.args {
            [Value::Number(item), Value::Number(count)] if request.player.key != 0 => {
                give(&giving, id(*item), id(*count), request.player)
            }
            _ => Outcome::refused(request.say("no-character", &[])),
        },
    )?;
    let giving = Arc::clone(tools);
    actions.add(
        admin("give-to")
            .arg(item)
            .arg(count)
            .arg(Arg::player("arg.player")),
        move |request| {
            let [
                Value::Number(item),
                Value::Number(count),
                Value::Player { key, .. },
            ] = request.args
            else {
                return Outcome::refused("");
            };
            match giving.world.players().into_iter().find(|p| p.key == *key) {
                Some(player) => give(&giving, id(*item), id(*count), &player),
                None => Outcome::refused(format!("no player {key:016x} online")),
            }
        },
    )?;
    Ok(())
}

/// Queues `count` of `item` for `player`.
fn give(
    tools: &Tools,
    item: u32,
    count: u32,
    player: &enshwick_loader_api::world::Player,
) -> Outcome {
    let Some(inventories) = tools.inventories else {
        return Outcome::refused(
            "giving items needs the inventory API, which this game build lacks",
        );
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
    Outcome::done(format!("giving {count} of item {item} to {}", player.name))
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
        if now == seen && !tail.waits() {
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

/// Runs a command file line on the console and logs its answer.
fn run(line: &str) {
    RUN.fetch_add(1, Ordering::Relaxed);
    match enshwick_loader_api::console::run(line) {
        Ok(answer) if answer.is_empty() => enshwick_loader_api::info!("{line}: no answer"),
        Ok(answer) => enshwick_loader_api::info!("{line}:\n{answer}"),
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
    fn actions_answer_administrators_while_on_and_file_lines_run_on_the_console() {
        use enshwick_loader_api::abi::ActionCall;
        use enshwick_loader_api::actions::sheet::decode;
        use enshwick_loader_api::console;

        let loader = enshwick_loader_api::test::FakeHost::new()
            .type_hash("keen::ecs::CurrentTransform", 1)
            .type_hash("keen::ecs::UsedItem", 2)
            .console_answer("enshwick", "Enshwick 0.0.0-test: admin-tools 0.1.0")
            .settings("enabled = false\n")
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);

        let admin = player(PlayerInfo::CAN_KICK_BAN);
        let sheet = decode(
            &loader
                .ask_actions(ActionCall::SHEET, &admin, 0, "", "")
                .unwrap(),
        );
        let ids: Vec<&str> = sheet.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "world", "entity", "placed", "mods", "reload", "give", "give-to"
            ]
        );
        assert!(sheet.iter().all(|e| !e.offered), "off until enabled");
        assert_eq!(
            loader
                .ask_actions(ActionCall::SHEET, &player(0), 0, "", "")
                .as_deref(),
            Some("")
        );
        let off = console::run("admin-tools world").unwrap();
        assert!(off.contains("enabled = true"), "{off}");

        loader.set_settings("enabled = true\n");
        assert_eq!(
            loader.raise::<enshwick_loader_api::events::SettingsChanged>(&()),
            1
        );
        assert_eq!(
            loader
                .ask_actions(ActionCall::RUN, &admin, 0, "world", "")
                .as_deref(),
            Some("1no players online")
        );
        assert_eq!(
            loader
                .ask_actions(ActionCall::RUN, &player(0), 0, "world", "")
                .as_deref(),
            Some("0You may not do that.")
        );
        assert!(
            console::run("admin-tools give 7 1")
                .unwrap()
                .contains("give-to")
        );
        assert!(
            console::run("admin-tools help")
                .unwrap()
                .contains("placed <1-32>")
        );
        assert!(loader.status()[0].starts_with("on; "));

        run("enshwick status");
        run("admin-tools world");
        assert_eq!(
            loader.console()[3..],
            ["enshwick status", "admin-tools world"]
        );
        let logs = loader.logs();
        assert!(logs.iter().any(|(_, l)| l.contains("Enshwick 0.0.0-test")));
        assert!(logs.iter().any(|(_, l)| l.contains("no players online")));
        assert!(loader.faults().is_empty());
    }

    #[test]
    fn a_hosted_game_s_next_world_starts_with_nothing_placed() {
        use enshwick_loader_api::abi::BuildingData;
        use enshwick_loader_api::events::ShutdownStarted;

        let loader = enshwick_loader_api::test::FakeHost::new()
            .type_hash("keen::ecs::CurrentTransform", 1)
            .type_hash("keen::ecs::UsedItem", 2)
            .settings(
                "enabled = true
",
            )
            .install();
        // SAFETY: the exports of `declare_mod!` above.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        let piece = BuildingData {
            header: Header::of::<BuildingData>(),
            entity: 900,
            player: 7,
            item: 5,
            kind: 0,
            replay: 0,
            template: [0; 16],
            position: [1.0, 2.0, 3.0],
        };
        assert_eq!(loader.raise::<BuildingPlaced>(&piece), 1);
        let placed = || enshwick_loader_api::console::run("admin-tools placed 5").unwrap();
        let before = placed();
        assert!(before.contains("900"), "{before}");
        assert!(loader.raise::<ShutdownStarted>(&()) >= 1);
        assert_eq!(placed(), "nothing placed since the world started");
    }
}
