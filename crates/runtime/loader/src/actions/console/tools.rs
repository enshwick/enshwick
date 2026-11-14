//! The console's tools: players, entities, placed pieces and mods, on the
//! game thread.

use std::sync::atomic::Ordering;

use enshwick_core::Entity;
use enshwick_core::abi::{PlayerInfo, Status};

use crate::engine::loader::Loader;
use crate::engine::texts;

use super::accounts;
use super::answers::{self, Loaded, Online, Seen};

/// A player's name as the client sent it, safe for one line.
pub(crate) fn name(player: &PlayerInfo) -> String {
    accounts::shown(&player.name_text())
}

fn position(loader: &Loader, entity: u32) -> Option<[f64; 3]> {
    crate::engine::world::fixed_position(loader, Entity(entity)).map(answers::metres)
}

/// `enshwick world`: the players online.
pub(crate) fn world(loader: &Loader) -> String {
    match crate::engine::world::players(loader) {
        Ok(players) => answers::world(
            &players
                .iter()
                .map(|p| Online {
                    name: name(p),
                    slot: p.slot,
                    entity: p.entity.0,
                    key: p.key.get(),
                    admin: p.is_admin(),
                    position: position(loader, p.entity.0),
                })
                .collect::<Vec<_>>(),
        ),
        Err(status) => unread(status),
    }
}

/// `enshwick entity <id>`: what the server knows of an entity.
pub(crate) fn entity(loader: &Loader, args: &str) -> String {
    let Ok(entity) = args.trim().parse::<u32>() else {
        return texts::fill(
            "console.usage",
            &[("usage", &texts::fill("usage.entity", &[]))],
        );
    };
    let player = crate::engine::world::players(loader)
        .ok()
        .and_then(|players| {
            players
                .iter()
                .find(|p| p.entity == Entity(entity))
                .map(name)
        });
    answers::entity(
        &Seen {
            entity,
            template: crate::engine::world::template(loader, Entity(entity)).ok(),
            item: crate::engine::world::used_item(loader, Entity(entity)),
            position: position(loader, entity),
            player,
        },
        |item| crate::content::recipes::item_words(loader, item),
    )
}

/// `enshwick placed [n]`: the pieces placed lately, newest first.
pub(crate) fn placed(loader: &Loader, args: &str) -> String {
    match answers::placed_count(args) {
        Ok(count) => answers::placed(&super::newest(count), |item| {
            crate::content::recipes::item_words(loader, item)
        }),
        Err(_) => texts::fill(
            "console.usage",
            &[("usage", &texts::fill("usage.placed", &[]))],
        ),
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
                running: !m.off.load(Ordering::Acquire),
            })
            .collect::<Vec<_>>(),
        &loader.skipped,
    )
}

/// Why the world did not read.
pub(super) fn unread(status: Status) -> String {
    texts::fill("console.unread", &[("status", &format!("{status:?}"))])
}

#[cfg(test)]
mod tests {
    use crate::base::resolve::{Skip, Skipped};

    #[test]
    fn enshwick_mods_lists_the_mods_the_loader_skipped_at_start() {
        let mut loader = crate::start::tests::bare_loader();
        assert_eq!(super::mods(&loader), "no mods loaded");
        loader.skipped.push(Skipped {
            id: "you.safes".into(),
            version: "0.2.0".into(),
            label: "Safes".into(),
            file: "you.safes/you.safes.dll".into(),
            reason: Skip::Missing {
                id: "you.locks".into(),
            },
        });
        assert_eq!(
            super::mods(&loader),
            "you.safes 0.2.0: skipped, you.safes/you.safes.dll: requires you.locks, which is not loaded"
        );
    }
}
