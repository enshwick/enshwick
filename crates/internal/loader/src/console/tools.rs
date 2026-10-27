//! The server administration tools: players, entities, placed pieces, mods,
//! settings reloads and the game's account actions, on the game thread.

use std::sync::atomic::Ordering;

use enshwick::abi::{PlayerInfo, Status};
use enshwick::ecs::Entity;

use crate::host::Loader;
use crate::texts;

use super::accounts::{self, Act, Refusal};
use super::answers::{self, Loaded, Online, Seen};

/// A player's name as the client sent it, safe for one line.
pub(crate) fn name(player: &PlayerInfo) -> String {
    let len = usize::from(player.name_len).min(player.name.len());
    accounts::shown(&String::from_utf8_lossy(&player.name[..len]))
}

pub(crate) fn is_admin(player: &PlayerInfo) -> bool {
    player.permissions & PlayerInfo::CAN_KICK_BAN != 0
}

fn position(loader: &Loader, entity: u32) -> Option<[f64; 3]> {
    crate::world::fixed_position(loader, Entity(entity)).map(answers::metres)
}

/// `enshwick world`: the players online.
pub(crate) fn world(loader: &Loader) -> String {
    match crate::world::players(loader) {
        Ok(players) => answers::world(
            &players
                .iter()
                .map(|p| Online {
                    name: name(p),
                    slot: p.slot,
                    entity: p.entity,
                    key: p.key,
                    admin: is_admin(p),
                    position: position(loader, p.entity),
                })
                .collect::<Vec<_>>(),
        ),
        Err(status) => unread(status),
    }
}

/// `enshwick entity <id>`: what the server knows of an entity.
pub(crate) fn entity(loader: &Loader, args: &str) -> String {
    let Ok(entity) = args.trim().parse::<u32>() else {
        return texts::fill("console.usage", &[("usage", "enshwick entity <number>")]);
    };
    let player = crate::world::players(loader)
        .ok()
        .and_then(|players| players.iter().find(|p| p.entity == entity).map(name));
    answers::entity(&Seen {
        entity,
        template: crate::world::template(loader, Entity(entity)).ok(),
        item: crate::world::used_item(loader, Entity(entity)),
        position: position(loader, entity),
        player,
    })
}

/// `enshwick placed [n]`: the pieces placed lately, newest first.
pub(crate) fn placed(args: &str) -> String {
    match answers::placed_count(args) {
        Ok(count) => answers::placed(&super::newest(count)),
        Err(_) => texts::fill("console.usage", &[("usage", "enshwick placed [number]")]),
    }
}

/// `enshwick mods`: each mod with its side, join need and state.
pub(crate) fn mods(loader: &Loader) -> String {
    answers::mods(
        &loader
            .mods
            .iter()
            .map(|m| Loaded {
                id: &m.id,
                version: &m.version,
                side: m.manifest.side,
                join: m.joins_when_on(),
                running: !m.off.load(Ordering::Acquire),
            })
            .collect::<Vec<_>>(),
    )
}

/// `enshwick reload <mod id>`: the mod reads its settings again.
pub(crate) fn reload(loader: &Loader, args: &str) -> String {
    let id = args.trim();
    let Some((_, record)) = loader.find_mod(&id.to_ascii_lowercase()) else {
        return texts::fill("enshwick.unknown", &[("id", id)]);
    };
    let Some(game) = loader.data_dir.parent() else {
        return texts::fill("enshwick.unknown", &[("id", id)]);
    };
    answers::reload(game, &record.id, crate::worlds::current().as_deref())
}

/// `enshwick kick|ban|unban <player or account>`: asks the game for its
/// account action on the player or account `args` names.
pub(crate) fn account(loader: &Loader, act: Act, args: &str) -> String {
    let online: Vec<accounts::Online> = match crate::world::players(loader) {
        Ok(players) => players
            .iter()
            .map(|p| accounts::Online {
                name: name(p),
                key: p.key,
                admin: is_admin(p),
            })
            .collect(),
        Err(status) => return unread(status),
    };
    let decided = accounts::target(args, &online).and_then(|target| accounts::decide(act, &target));
    let (key, who) = match decided {
        Ok(decided) => decided,
        Err(refusal) => return refused(act, &refusal),
    };
    let (asked, verb) = match act {
        Act::Kick => ("console.kick.asked", "kick"),
        Act::Ban => ("console.ban.asked", "ban"),
        Act::Unban => ("console.unban.asked", "unban"),
    };
    match super::admin::act(loader, act, key) {
        Ok(()) => {
            crate::log!(INFO, "the console asked the game to {verb} {who}");
            texts::fill(asked, &[("who", &who)])
        }
        Err(Status::MISSING) => texts::fill("console.account.missing", &[]),
        Err(status) => unread(status),
    }
}

/// What the console answers when it asks the game for nothing.
fn refused(act: Act, refusal: &Refusal) -> String {
    match refusal {
        Refusal::Empty => {
            let usage = match act {
                Act::Kick => "enshwick kick <player or account>",
                Act::Ban => "enshwick ban <player or account>",
                Act::Unban => "enshwick unban <account>",
            };
            texts::fill("console.usage", &[("usage", usage)])
        }
        Refusal::Unknown(word) => texts::fill("console.account.unknown", &[("word", word)]),
        Refusal::Shared(word, keys) => texts::fill(
            "console.account.shared",
            &[("word", word), ("accounts", &accounts::listed(keys))],
        ),
        Refusal::Admin(name) => texts::fill("console.account.admin", &[("name", name)]),
        Refusal::Offline(key) => texts::fill(
            "console.account.offline",
            &[("account", &format!("{key:016x}"))],
        ),
    }
}

/// Why the world did not read.
pub(super) fn unread(status: Status) -> String {
    texts::fill("console.unread", &[("status", &format!("{status:?}"))])
}
