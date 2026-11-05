# What players do with items

A mod hears a player use, trash, equip, unequip, pick up or place an item,
may refuse the first four, and hears that every mod allowed a use. The calls are
[unstable](../stability.md) until a person checks them in game.

## Give an item behaviour

[`load.on_item(item, kind, handler)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/items.rs)
runs `handler` when a player does `kind` with `item`. A mod's own item is
`ItemId::from_guid` of its content id's GUID, and in Luau
`enshwick.content.item_id` of its full content id. The handler gets an
`ItemAction`: the `kind`, the `player`'s character, the `item`, the `count`
and the `slot` it is in before the action. Returning `false` refuses the
action where the kind allows it.

```rust
use enshwick::content::{Id, id};
use enshwick::items::Kind;

const TONIC: Id = id!("tonic");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let tonic = enshwick::ItemId::from_guid(TONIC.guid());
    load.on_item(tonic, Kind::Consume, |used| {
        enshwick::info!("player {} drank a tonic", used.player.0);
        true
    })?;
    // Nobody throws a tonic away.
    load.on_item(tonic, Kind::Trash, |_| false)
}
```

```luau
local tonic = enshwick.content.item_id("you.tonics:tonic")
enshwick.items.on_item(tonic, "Consume", function(used)
    enshwick.log.info(`player {used.player} drank a tonic`)
end)
-- Nobody throws a tonic away.
enshwick.items.on_item(tonic, "Trash", function() return false end)
```

A Luau handler that returns nothing allows the action.
[`load.on_item_action(filter)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/items.rs)
hears the actions with every item, for a mod that decides by the item.

## Bind a function in `items.toml`

A [data pack](packs.md) with a `main.luau` names the item's functions in the
item's `on` table, one key per kind. Each value is the name of a global
function of `main.luau`, which the loader binds once `main.luau` ran, as
`enshwick.items.on_item` binds it.

```toml
[[item]]
id = "tonic"
like = "cd7c8239-51ad-4e08-a90d-642f8b8616a1" # Consumable_Potion_Health_T1
name = "tonic.name"
on = { consume = "drink_tonic", trash = "keep_tonic" }
```

```luau
function drink_tonic(used)
    enshwick.log.info(`player {used.player} drank a tonic`)
end

function keep_tonic()
    return false
end
```

`pack check` reports a key other than the seven kinds, a value that is no
Luau name, and an `on` table in a pack without a `main.luau`. A name that
`main.luau` leaves without a global function stops the mod at start, with
the item and the key in the log.

## Which actions a mod hears

| Kind      | `on` key  | When                                            | What a refusal does                |
| --------- | --------- | ----------------------------------------------- | ---------------------------------- |
| `Consume` | `consume` | A player eats, drinks or uses the item          | The item stays in its slot         |
| `Trash`   | `trash`   | A player throws the stack away                  | The stack stays                    |
| `Equip`   | `equip`   | A player equips the item                        | The equipment stays as it was      |
| `Unequip` | `unequip` | A player takes the item off                     | The item stays on                  |
| `Pickup`  | `pickup`  | After a player picked the item up               | Nothing: the answer is ignored     |
| `Place`   | `place`   | After a player placed the item as a piece       | Nothing: the answer is ignored     |
| `Consumed` | `consumed` | After every mod allowed a `Consume`           | Nothing: the answer is ignored     |

A mod that counts uses or rewards one listens to `Consumed`: a `Consume`
reaches the mods before they all decided, so another mod may still refuse
it. `Consumed` comes before the game runs the use, so a use the game
itself then drops still counts.

A refusal takes the game's path for an action it already ran, so the item
stays where it was and nothing is lost. `count` is 1 for a use and a
placement, and the stack for the others; `slot` is none for a pickup and a
placement.

The source is the reflection of dedicated server build 26319703: a use and a
trash are the player input's `itemAction`, which the server's
`inventory_actions` system runs, an equip and an unequip its
`changeEquipmentAction`, which `equipment_actions` runs, and a pickup and a
placement the `PickupEvent` and the `BuildingPlaceEvent`. A use from the
action bar and a tool's hit are not heard yet.

## Where the handlers run

The handlers run in the server's world only, in the dedicated server or a
hosting client: a client playing on a server needs no mod for them. A use,
a trash, an equip and an unequip reach them on the thread of the system
that runs the action, once per action; a pickup and a placement on the game
thread, before the next step. Keep them quick and never block. Several mods
may hear one item; any refusal wins, and an action after the fact reaches
every mod. A handler that panics, or a Luau handler that fails, allows the
action and turns its mod off.
