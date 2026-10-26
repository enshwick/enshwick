# Inventories

[`enshwick::inventory`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)
reads what a chest, a corpse or a player holds, and changes it through the
game's own transactions. It works in the game server: the dedicated server,
or a client while it hosts a game.

Ask for it during `init` with
[`load.inventories()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs).
The returned
[`Inventories`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)
is `Copy`; keep it anywhere.

```rust
use enshwick::ecs::Entity;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let inventories = load.inventories()?;
    let (corpse, player) = (Entity(40), Entity(2));
    load.scheduler()?.next_tick(move || {
        let full = inventories.read().slots(corpse).iter().filter(|s| !s.is_empty()).count();
        enshwick::info!("{full} stacks to loot");
        inventories.move_all(corpse, player, |outcome| {
            enshwick::info!("looted: {}", outcome.is_committed());
        });
    });
    Ok(())
}
```

`load.inventories()` returns `Err(Off)` after `init` or on a game build that lacks a piece the reads or the
transactions need.

## How inventories are built

Items are not entities. An inventory, such as a chest or a player, links up
to 16 sub-inventories of 8 slots each; a link may be virtual, pointing at
another inventory. Only items with durability or a level have an entity of
their own, the item's PIDE.

A slot is a
[`SlotRef`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs):
the sub-inventory entity and the slot index, packed as the game packs them,
`entity | index << 32`.

```rust
use enshwick::ecs::Entity;
use enshwick::inventory::SlotRef;

let slot = SlotRef::new(Entity(0x41), 3);
assert_eq!(slot.0, 0x3_0000_0041);
assert_eq!((slot.inventory(), slot.index()), (Entity(0x41), 3));
```

## Reads

Reads are synchronous plain memory reads. Make them where no inventory
changes underneath:

- [`read()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)
  on the server's game thread between steps: in a game event, a
  [scheduler](events.md) task or an inventory callback. Elsewhere it returns
  nothing: empty lists, zero counts, `None`;
- [`read_in(ctx)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)
  inside a hooked ECS system, through its context. It is `unsafe`: `ctx`
  must be the hooked system's live context, used on its thread during the
  hook.

Both return a
[`Reader`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs):

| Call | Returns |
| --- | --- |
| `slots(root)` | Every slot of the inventory `root`, virtual links followed, empty slots included, in the game's order; empty when `root` has no inventory. |
| `count(root, item)` | How many of item id `item` the inventory holds. |
| `slot(slot_ref)` | What one slot holds, with `InventoryCategory::INVALID`. |
| `root(slot_ref)` | The inventory a slot belongs to at the root, the chest or player; `None` when its sub-inventory has no owner. |
| `durability(pide)` | An item entity's durability as clients see it: a `Bar` with `current` and `max`; `None` for an item without durability. |
| `meter(pide)` | An item entity's [meter](content/items.md#meters), a `Bar`; `None` for an item without one. |

A [`Slot`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)
holds `at` (its `SlotRef`), `category` (equipment, currency, ordinary
storage, ...), `item` (the item id, 0 for none), `count` and `pide`
(`None` for items without an entity). `is_empty()` is true when
`item` or `count` is 0.

Item ids are [`Guid::id`](content/items.md#guids-and-item-ids) of the
item's GUID.

[`item_info(item)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)
gives an item's static data from any thread: `max_stack_size`, and flags with
`has_pide()`, `is_container()` and `is_blueprint()`. It returns `None` for
an unknown item, and before the game first ran its inventory system, which
needs a player online.

## Changes

Changes are requests. Submit them from any thread; the loader runs each in
the game's inventory system as one transaction, and hands the outcome to
your callback later.

One operation each:

| Call | Does |
| --- | --- |
| `give(to, item, count, done)` | Gives new items: merged into stacks of the same item, then free slots. |
| `take(from, item, count, done)` | Removes items, from whichever stacks hold them. |
| `destroy_slot(slot, done)` | Destroys a stack. |
| `move_slot(slot, to, amount, done)` | Moves a stack, or `Some(amount)` of it, into an inventory's free or matching slots. |
| `move_all(from, to, done)` | Moves every stack of one inventory into another, as the game's "take all" does. |

`Op::Give` in a batch also takes a `fill`: how full each new item's
[meter](content/items.md#meters) starts, as a `Fill`. `Fill::of(count, max)`
gives `count` of `max`; `None` keeps the template's start.

Moves leave what does not fit in the source. `move_all` leaves the source's
action bar, equipment and locked slots where they are;
`Op::MoveAll { include_special: true, .. }` in a batch moves them too.

`Op::Renew` replaces a stack with new items, each with its own entity and
its [meter](content/items.md#meters) at a fill, asked for the same slot: an
item from before its mod gave it a meter gets one at once.
`renew(root, items, fill, done)` renews every such stack of `items` in an
inventory in one transaction, and returns `None` when none needs it.

### Batches

[`Batch`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)
runs several [`Op`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)s
as one transaction through
[`submit`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs):
every operation succeeds, or nothing changes.

```rust
use enshwick::ecs::Entity;
use enshwick::inventory::{Batch, Inventories, Op};

const COIN: u32 = 0x0c01;
const SWORD: u32 = 0x5e01;

fn trade(inventories: &Inventories, player: Entity) {
    // 5 coins out, a sword in, or neither.
    let batch = Batch::new()
        .with(Op::Take { from: player, item: COIN, count: 5 })
        .with(Op::Give { to: player, item: SWORD, count: 1, fill: None });
    let _ticket = inventories.submit(&batch, |outcome| {
        enshwick::info!("trade: {:?}", outcome.done);
    });
}
```

The one-operation calls above are batches of one. A batch also sets:

- `allow_partial()`: commit even when a give or take leaves some of its
  count undone; without it, that rolls the batch back;
- `notify()`: show the actor the game's pickup message for each stack and
  its inventory-full message for what does not fit, as the game's own
  looting does. A batch with a give always does; moves are silent without
  it;
- `actor(player)`: the player the game credits with the change, for those
  messages. By default it is the destination of the first give or move.

### Limits and splitting

A transaction has limits, and
[`Batch::check`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)
tests them:

- at most 64 operations
  ([`MAX_OPS`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs));
- at most 16 distinct inventories named
  ([`MAX_INVENTORIES`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs));
- what fits the game's op log of 512 records, counting a give or take as 16,
  a slot move as 4, a destroy as 1 and a `MoveAll` as 256. So one batch
  holds at most two `MoveAll`s;
- no malformed operation: no inventory or item, a count of 0 or above
  `i32::MAX`, a move amount of `Some(0)`, a `MoveAll` from an inventory into
  itself.

`submit` refuses a batch that breaks a limit before it reaches the game,
with `Done::Refused(BatchError)`.
[`Batch::split`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)
cuts a batch into parts that fit, in order, keeping its settings. Each part
is all or nothing on its own; together they are not.

```rust
use enshwick::ecs::Entity;
use enshwick::inventory::{Batch, Op};

let mut everything = Batch::new();
for chest in 100..120 {
    everything.push(Op::MoveAll { from: Entity(chest), to: Entity(2), include_special: false });
}
let parts = everything.split();
assert!(parts.len() > 1);
assert!(parts.iter().all(|part| part.check().is_ok()));
```

## Outcomes

Each callback gets an
[`Outcome`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs):
`done`, how the request ended, and `ops`, one result per operation when the
request ran, else empty.

| `Done` | Meaning |
| --- | --- |
| `Committed` | Every operation succeeded; the change is made. |
| `RolledBack` | An operation failed or fell short; nothing changed. |
| `Unavailable` | The build lacks a piece, the loader's hook is not in place, or a client hosts no game; nothing changed. |
| `TimedOut` | The game did not run its inventory system within 30 seconds; nothing changed. |
| `Refused(BatchError)` | The batch broke a limit and never reached the game. |

An [`OpResult`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)
holds the game's `error` code (0 for none) and `remaining`, how much of the
count the operation could not do. `ok()` is true without an error.
Operations after a failed one are not run and carry
`InventoryOpResult::NOT_RUN`. `OpResult::NO_ROOM` marks an item that did not
fit or does not exist, `OpResult::TOO_LARGE` a transaction past the game's
limits.

```rust
use enshwick::inventory::{Done, Outcome};

fn report(outcome: &Outcome) {
    match outcome.done {
        Done::Committed => enshwick::info!("done"),
        Done::RolledBack => {
            let failed = outcome.ops.iter().position(|op| !op.ok() || op.remaining > 0);
            enshwick::warn!("rolled back at operation {failed:?}");
        }
        other => enshwick::warn!("not run: {other:?}"),
    }
}
```

After a transaction, committed or not, the loader has the game resend every
inventory it touched to the clients, as vanilla does after a transfer.

### Tickets

Every request returns a
[`Ticket`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/inventory.rs)
that holds the same outcome once it is known, for polling instead of a
callback: `is_done()` and `outcome()`. Clone it freely. A callback that does
nothing, `|_| {}`, is fine when you poll.

## Callbacks and threads

- `submit` and the one-operation calls work from any thread and return at
  once.
- The loader runs requests inside the game's inventory system, one at a
  time, in the order they came.
- The callback runs on the server's game thread after a later step, so it
  may [read](#reads) inventories and submit further requests, such as to empty one container after another.
- A refused batch, and a request the loader turns away, get their outcome
  the same way, on the game thread after the next step.
- If the mod is turned off before the outcome arrives, the callback never
  runs. A panic in a callback is caught and reported as the mod's fault.

The inventory system runs once per online player each step. With nobody
online it does not run at all, and requests wait until they time out after
30 seconds.

## On a hosting client

A client that hosts a game runs the inventory system in the built-in
server's world, and in its own. The loader runs requests in the server's
world only. A client that hosts nothing turns requests away with
`Unavailable`; the outcome arrives once the scheduler runs again, when a
game is hosted. See [Sides and hosting](concepts/sides-and-hosting.md).

## Testing

[The test fake](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/test.rs)
records a mod's requests: `inventory_requests()` lists them and
`finish_inventory(outcome, results)` answers the oldest as the game would;
`item` and `without_inventories` set what the build has. See
[Testing](testing.md).

[`enshwick-area-loot`](https://github.com/enshwick/mod-area-loot/blob/main/src/take.rs)
uses reads, `MoveAll` batches with `actor`, `allow_partial` and `notify`,
and chained callbacks; see [Loot containers](content/loot.md#a-worked-example).
