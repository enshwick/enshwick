# Interactions

A mod has a say before the game lets a player interact with an entity.
[`load.on_interaction`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/interactions.rs)
runs a filter with an `Interaction`: what the player does (`kind`), the
player, the target, and the item the target was placed from and where it
stands, when the platform could read them. Returning `false` refuses it.

```rust
use enshwick_loader_api::interactions::Kind;

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    const LOCKED_CHEST: u32 = 0x5e01;
    load.on_interaction(|i| !(i.item == Some(LOCKED_CHEST) && matches!(i.kind, Kind::Open | Kind::Move | Kind::Sort)))
}
```

| Kind         | When it is asked                                          | What a refusal does              |
| ------------ | --------------------------------------------------------- | -------------------------------- |
| `Open`       | The game is about to offer the Open prompt                | No prompt                        |
| `Move`       | Items move out of or into the target, or one of its slots | The move is undone on the client |
| `Sort`       | The target's inventory is sorted                          | Nothing is sorted                |
| `Stock`      | The target lends its stock to crafting or building nearby | It is left out of the stock      |
| `Dismantle`  | The cursor offers to pick up or tear down the target      | No pick-up or tear-down          |
| `QuickStash` | The player flips the target's Quick Stash switch          | The switch stays                 |
| `Upgrade`    | The player upgrades the target into another piece         | Nothing is upgraded              |

Each refusal takes the game's own path for a refused action, so nothing moves
and nothing is lost. `Stock` is asked without the target's item or position:
answer it from what the mod has already seen of the target.

An interaction without a player's character or without a target is allowed
without asking. The filters run in the server's world only, in the dedicated server or a
hosting client, on the thread of the system that checks the interaction: keep
them quick and never block. Several mods may filter; any refusal wins.
