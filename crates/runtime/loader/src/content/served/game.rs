//! The game archive's resource keys and the GUIDs they hold, read once.

use std::sync::OnceLock;

use enshwick_core::abi::ResourceKey;
use enshwick_core::content::Guid;

/// Every resource key of the game's archive, in archive order, read once;
/// `None` when the archive does not read.
pub(crate) fn game_keys() -> Option<&'static [ResourceKey]> {
    game().map(|game| &*game.keys)
}

/// Whether the game's archive holds content `guid`, the game's own;
/// `false` when the archive does not read.
pub(crate) fn game_holds(guid: Guid) -> bool {
    game().is_some_and(|game| game.guids.contains(&guid))
}

struct GameKeys {
    keys: Vec<ResourceKey>,
    guids: rustc_hash::FxHashSet<Guid>,
}

fn game() -> Option<&'static GameKeys> {
    GAME_KEYS
        .get_or_init(|| {
            crate::engine::archive::game().map(|archive| {
                let keys = archive.keys();
                let guids = keys.iter().map(|key| key.guid).collect();
                GameKeys { keys, guids }
            })
        })
        .as_ref()
}

static GAME_KEYS: OnceLock<Option<GameKeys>> = OnceLock::new();
