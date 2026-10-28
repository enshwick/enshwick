# Random loot

`load.add_loot(drop)` puts an item into random loot with an exact chance per
roll, on top of the vanilla loot. The item is a vanilla one or one from
[`add_item`](items.md); the game skips an item its registry does not hold.
The API is in
[`loot/drops.rs`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/loot/drops.rs).

```rust
use enshwick::content::Id;
use enshwick::loot::{Drop, Kinds, Source};

const STAFF: Id = Id::new("my-mod:staff");
const RING: Id = Id::new("my-mod:ring");
const AMULET: Id = Id::new("my-mod:amulet");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.add_loot(Drop::new(enshwick::ItemId::from_guid(STAFF.guid()), 0.10, Source::bosses()))?;
    load.add_loot(Drop::new(enshwick::ItemId::from_guid(STAFF.guid()), 0.02, Source::locked_chests()))?;
    load.add_loot(
        Drop::one_of(&[enshwick::ItemId::from_guid(RING.guid()), enshwick::ItemId::from_guid(AMULET.guid())], 0.01, Source::kinds(Kinds::CORPSES))
            .count(1, 2),
    )
}
```

## When loot rolls

- The game rolls a container's loot once, when a player first opens it:
  world chests and enemy corpses. Each matching drop then rolls its own
  chance and adds its stack in the same transaction as the vanilla loot, so
  it is saved and replicated with it.
- Containers rolled before, in a save, stay as they were.
- Destroyed objects and pickup spawners roll a single item instead. A drop
  for `Kinds::DESTROYED` that hits takes the place of that item.
- A full container keeps what fits.
- Loot rolls on the server: the dedicated one, or a client's while it hosts.

## A drop

A [`Drop`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/loot/drops.rs)
holds the items, the chance, the count and the source:

- `Drop::new(item, chance, from)` gives one item, `item` being the game id
  [`ItemId::from_guid`](items.md#guids-and-item-ids) gives;
- `Drop::one_of(items, chance, from)` gives one of several items, each as
  likely; `chance` is that of any of them;
- `.count(min, max)` gives `min` to `max` items per hit, each count in
  between as likely; one by default;
- `.meter(min, max, of)` starts each dropped item's
  [meter](items.md#meters) at `min` to `max` of `of`, each count as likely:
  charges rolled as the item drops. Without it the item starts as its
  template says. A single-item roll, the `DESTROYED` kind, keeps the
  template's start.

`add_loot` turns the mod off when the drop is not valid: no item, an item id
of 0, a chance outside 0 to 1, a count with `min` of 0 or above `max`, or a
meter with `min` above `max`, `max` above `of` or an `of` of 0. It
also does so when the running build lacks a piece the drops need, such as the
pinned `loot.roll` symbol, or `loot.roll_one` for a `DESTROYED` drop.

`loot::replace_drops(drops)` swaps the mod's drops while the game runs, from
any thread, such as when a setting changes; an empty list stops them. A kind of
roll that no drop linked at start waits for a restart, so a mod that may turn
its drops on later adds them at init and replaces them with none at once.

## Where it drops

`Source` filters the rolls a drop takes part in:

| Filter                         | Matches                                          |
| ------------------------------ | ------------------------------------------------ |
| `Source::kinds(kinds)`         | every container of those kinds                   |
| `Source::bosses()`             | bosses' corpses only                             |
| `Source::locked_chests()`      | chests that need a key or a lockpick only        |
| `.with_labels(&[id, ...])`     | the same, limited to these loot label ids        |
| `.with_nodes(&[node, ...])`    | the same, limited to these world nodes           |

`Kinds` are `CORPSES` (corpses and loot bags enemies leave), `CHESTS` (world
chests and every other container that rolls on first opening), `DESTROYED`
and `CONTAINERS` (corpses and chests); `with` joins two sets.

Label ids and node ids are `LootContainerId`s, as the game names them; a label id is the FNV-1a hash of an enemy or chest type's label
collection GUID. `Source::matches` tells whether a
[`Roll`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/loot/drops.rs)
comes from a source, which a unit test can check without the game:

```rust
use enshwick::Entity;
use enshwick::loot::{Kinds, LootContainerId, Roll, Source};

let boss = Roll { kind: Kinds::CORPSES, entity: Some(Entity(7)), node: None, labels: Some(LootContainerId::new(9)), boss: true, locked: false };
assert!(Source::bosses().matches(&boss));
assert!(!Source::locked_chests().matches(&boss));
assert!(Source::kinds(Kinds::CORPSES).with_labels(&[LootContainerId::new(9)]).matches(&boss));
```

## A worked example: copy staffs

[Survivors per altar](https://github.com/enshwick/mod-survivors-per-altar)
adds a copy staff per survivor with `add_item`, then puts them in loot with
`Drop::one_of`: one chance for bosses and one for locked chests, both from
its settings, and no drop when a chance is 0. When `copy_staffs` turns off
while the game runs, it calls `replace_drops` with none.
