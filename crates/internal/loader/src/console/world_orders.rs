//! The console's orders on the world through the same game calls the
//! game's admin orders use: the inventory add, the spawn request and the
//! teleport, on the game thread.

use core::ffi::c_void;

use enshwick::abi::{
    Header, InventoryOp, InventoryRequest, InventoryResult, PlayerInfo, SpawnRequest, Status,
    TeleportRequest,
};
use enshwick::content::Guid;
use enshwick::ecs::Entity;

use crate::host::Loader;
use crate::mod_index::ModIndex;
use crate::texts;

use super::accounts::{self, Refusal, Target};
use super::answers;
use super::orders::{self, Place, Recipients, Usage};
use super::{Order, tools};

/// How far beside a player a spawn lands, in metres, and how far apart
/// several spawns stand.
const BESIDE: f64 = 2.0;

/// `enshwick give|give-to|spawn|teleport ...`.
pub(crate) fn order(loader: &Loader, order: Order, args: &str) -> String {
    let answer = match order {
        Order::Give => orders::give(args).map(|give| self::give(loader, &give)),
        Order::GiveTo => orders::give_to(args).map(|give| self::give(loader, &give)),
        Order::Spawn => orders::spawn(args).map(|spawn| self::spawn(loader, &spawn)),
        Order::Teleport => orders::teleport(args).map(|to| teleport(loader, &to)),
    };
    answer.unwrap_or_else(|Usage(usage)| texts::fill("console.usage", &[("usage", usage)]))
}

/// The player online `words` name, or the answer that says why none.
fn player(loader: &Loader, words: &str) -> Result<PlayerInfo, String> {
    let players = crate::world::players(loader).map_err(tools::unread)?;
    let online: Vec<accounts::Online> = players
        .iter()
        .map(|p| accounts::Online {
            name: tools::name(p),
            key: p.key,
            admin: tools::is_admin(p),
        })
        .collect();
    let key = match accounts::target(words, &online) {
        Ok(Target::Player(found)) => found.key,
        Ok(Target::Account(key)) => {
            return Err(texts::fill(
                "console.order.offline",
                &[("account", &format!("{key:016x}"))],
            ));
        }
        Err(Refusal::Shared(word, keys)) => {
            return Err(texts::fill(
                "console.account.shared",
                &[("word", &word), ("accounts", &accounts::listed(&keys))],
            ));
        }
        Err(Refusal::Unknown(word)) => {
            return Err(texts::fill("console.account.unknown", &[("word", &word)]));
        }
        Err(_) => return Err(texts::fill("console.account.unknown", &[("word", words)])),
    };
    players
        .into_iter()
        .find(|p| p.key == key && p.entity != 0)
        .ok_or_else(|| {
            texts::fill(
                "console.order.offline",
                &[("account", &format!("{key:016x}"))],
            )
        })
}

/// What a give is for, kept until the game answers it.
struct Gift {
    who: String,
    item: u32,
    count: u32,
}

/// Logs what the game did with a console give; takes back the [`Gift`]
/// that [`give`] leaked into `user`.
unsafe extern "system" fn given(result: *const InventoryResult, user: *mut c_void) {
    // SAFETY: `user` is the `Box<Gift>` `give` turned into a raw pointer for
    // this request, and the inventory delivery calls this once per request.
    let gift = unsafe { Box::from_raw(user.cast::<Gift>()) };
    // SAFETY: the delivery passes its own `InventoryResult`, valid for the
    // call.
    let outcome = unsafe { result.as_ref() }.map(|r| r.outcome);
    let Gift { who, item, count } = *gift;
    if outcome == Some(InventoryResult::COMMITTED) {
        crate::log!(INFO, "the console gave {count} of item {item} to {who}");
    } else {
        crate::log!(
            WARN,
            "the console's {count} of item {item} for {who} were not given ({outcome:?}): \
             an unknown item, a full inventory or no inventory system"
        );
    }
}

/// Queues `give` for each player it names, one request each, so one full
/// inventory refuses only its own.
fn give(loader: &Loader, give: &orders::Give<'_>) -> String {
    let players = match give.to {
        Recipients::Everyone => match crate::world::players(loader) {
            Ok(players) => players.into_iter().filter(|p| p.entity != 0).collect(),
            Err(status) => return tools::unread(status),
        },
        Recipients::Named(words) => match player(loader, words) {
            Ok(player) => vec![player],
            Err(answer) => return answer,
        },
    };
    if players.is_empty() {
        return texts::fill("console.give.nobody", &[]);
    }
    let mut asked = Vec::new();
    for player in &players {
        let who = format!("{} ({:016x})", tools::name(player), player.key);
        let op = InventoryOp {
            kind: InventoryOp::GIVE,
            flags: 0,
            target: u64::from(player.entity),
            other: 0,
            item: give.item,
            count: give.count,
        };
        let request = InventoryRequest {
            header: Header::of::<InventoryRequest>(),
            flags: InventoryRequest::NOTIFY,
            actor: player.entity,
            ops: &raw const op,
            op_count: 1,
            reserved: 0,
        };
        let gift = Box::into_raw(Box::new(Gift {
            who: who.clone(),
            item: give.item,
            count: give.count,
        }));
        let status =
            crate::inventory::submit(loader, ModIndex::LOADER, &request, given, gift.cast());
        if !status.is_ok() {
            // SAFETY: the request was refused, so `given` never runs for it
            // and `gift` is still the box made above.
            drop(unsafe { Box::from_raw(gift) });
            return if status == Status::MISSING {
                texts::fill("console.give.missing", &[])
            } else {
                tools::unread(status)
            };
        }
        asked.push(who);
    }
    texts::fill(
        "console.give.asked",
        &[
            ("count", &give.count.to_string()),
            ("item", &give.item.to_string()),
            ("who", &asked.join(", ")),
        ],
    )
}

/// Spawns `spawn.count` entities from its template, a step apart along x.
fn spawn(loader: &Loader, spawn: &orders::Spawn<'_>) -> String {
    let origin = match spawn.at {
        Place::Point(point) => point,
        Place::Beside(words) => {
            let player = match player(loader, words) {
                Ok(player) => player,
                Err(answer) => return answer,
            };
            let Some([x, y, z]) =
                crate::world::fixed_position(loader, Entity(player.entity)).map(answers::metres)
            else {
                return texts::fill("console.order.nowhere", &[("who", &tools::name(&player))]);
            };
            [x + BESIDE, y, z]
        }
    };
    let mut entities = Vec::new();
    let mut stopped = None;
    for i in 0..spawn.count {
        let [x, y, z] = origin;
        let request = SpawnRequest {
            header: Header::of::<SpawnRequest>(),
            template: spawn.template,
            position: enshwick::ecs::fixed([x + BESIDE * f64::from(i), y, z]),
            orientation: [0.0, 0.0, 0.0, 1.0],
            level: 0,
            reserved: 0,
            lifetime_ns: 0,
        };
        match crate::spawns::spawn(loader, &request) {
            Ok(entity) => entities.push(entity.to_string()),
            Err(status) => {
                stopped = Some(status);
                break;
            }
        }
    }
    let why = match stopped {
        None => None,
        Some(Status::MISSING) => Some(texts::fill("console.spawn.missing", &[])),
        Some(status) => Some(tools::unread(status)),
    };
    if entities.is_empty() {
        return why.unwrap_or_default();
    }
    crate::log!(
        INFO,
        "the console spawned {} at {}: entities {}",
        Guid(spawn.template),
        answers::place(Some(origin)),
        entities.join(", ")
    );
    let asked = texts::fill(
        "console.spawn.asked",
        &[
            ("template", &Guid(spawn.template).to_string()),
            ("place", &answers::place(Some(origin))),
            ("entities", &entities.join(", ")),
        ],
    );
    match why {
        Some(why) => format!("{asked} {why}"),
        None => asked,
    }
}

/// Moves the named player's character.
fn teleport(loader: &Loader, order: &orders::Teleport<'_>) -> String {
    let player = match player(loader, order.who) {
        Ok(player) => player,
        Err(answer) => return answer,
    };
    let request = TeleportRequest {
        header: Header::of::<TeleportRequest>(),
        entity: player.entity,
        flags: 0,
        position: enshwick::ecs::fixed(order.to),
        orientation: [0.0, 0.0, 0.0, 1.0],
    };
    let who = format!("{} ({:016x})", tools::name(&player), player.key);
    match crate::emits::teleport(loader, &request) {
        Ok(()) => {
            crate::log!(
                INFO,
                "the console moved {who} to {}",
                answers::place(Some(order.to))
            );
            texts::fill(
                "console.teleport.asked",
                &[("who", &who), ("place", &answers::place(Some(order.to)))],
            )
        }
        Err(Status::MISSING) => texts::fill("console.teleport.missing", &[]),
        Err(Status::REFUSED) => texts::fill("console.teleport.refused", &[("who", &who)]),
        Err(status) => tools::unread(status),
    }
}
