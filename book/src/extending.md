<!--
type: how-to
description: A mod lets other mods change one of its rules at a typed extension point: it asks a question, answers it first, and every linked mod may change the answer.
-->

# Extending other mods

A mod can let other mods change one of its rules at an **extension point**.
A point is a typed question the mod asks whenever the rule applies. The mod
answers first, and each other mod's link follows.

## How links work

Links attach in order and may change the answer, or refuse where the point
allows. When the game starts, each link is checked against the point's
declaration field by field, like [services](services.md). A link that panics
is turned off alone. The player can switch one link off without the mod. The
Mods screen and the [conflict report](logging.md#conflicts) show who extends
what.

The [examples](https://github.com/enshwick/enshwick/tree/main/examples)
folder holds the mods this chapter follows. `trading-post` defines the point
`you.trading-post:price`, what it pays for an item. `haggling` extends it.
Regular customers get more for what they sell, and the trading post buys no
item on the refused list.

## Getting a point's definition

A mod that extends a point compiles a definition of it: the input struct, the
answer's type and every type they reach. The owner's build writes a schema of
each point, call and mod event it lists in its manifest. `cargo enshwick deps`
writes the definitions of everything a dependency offers from it, into
`src/deps/<alias>.rs`:

```bash
cargo enshwick deps trading_post --from <game folder>
```

The alias is the one the mod's `Cargo.toml` gives the owner. It sits in
`[package.metadata.enshwick.requires]` or `after`. Without an alias, the tool
writes a file for every dependency. Each definition names the point by its
local name and the alias, as
`#[enshwick::point(name = "price", of = trading_post, ..)]`. `--from` names
the owner's DLL or a schema file the owner published. It may also name a
folder with the DLL in it or in its `mods/`, such as the game's. The tool
reads the schema from the DLL without running it. `enshwick::deps!()` at the
crate root makes each file the public module `deps::<alias>`, so its types
count as the mod's own API. The file ends with checks that hold it to the
owner's layout. When the owner adds fields or points, write the file again
rather than editing it. A definition older than the owner keeps working,
since points grow by appending fields. A definition that no longer matches
the owner leaves the link inert, with the reason in the log.

### The schema

The schema is TOML, the same for every language a definition is written in:

| Table                                | Key                       | What it holds                |
| ------------------------------------ | ------------------------- | ---------------------------- |
| top                                  | `schema`                  | The format, 1                |
| top                                  | `mod`                     | The owner's id               |
| top                                  | `version`                 | The owner's version          |
| `[[point]]`                          | `name`, `mode`            |                              |
| `[[point]]`                          | `refusable`, `input`      |                              |
| `[[point]]`                          | `output`, `doc`           |                              |
| `[[point]]`                          | `layout`, `output_layout` |                              |
| `[[type]]`                           | `name`, `doc`             |                              |
| `[[type]]`                           | `kind`                    |                              |
| `[[type]]`                           | `repr`                    | Only for `values` and `enum` |
| `[[type.field]]`                     | `name`, `doc`             |                              |
| `[[type.field]]`                     | `type`                    |                              |
| `[[type.value]]`, `[[type.variant]]` | `name`, `doc`             |                              |
| `[[type.value]]`, `[[type.variant]]` | `value`                   |                              |

A point's `mode` is one of three values:

- `observe`
- `advise`
- `override`

A type's `kind` is one of four values:

- `struct`
- `newtype`
- `values`
- `enum`

A type in the schema is one of these:

- an unsigned integer, `u8` to `u64`
- a signed integer, `i8` to `i64`
- a float number: `f32` or `f64`
- `bool`
- `enshwick.entity` or `enshwick.guid`
- a `[[type]]` of the schema

`layout` lists each input field's layout hash and `output_layout` the
answer's, as decimal text: a definition checks itself against them.

## Extending a point

[`load.extend_point(order, link)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/points.rs)
attaches `link`. A mod calls it during `init`. The link gets the point's input and
the answer so far. That answer is the owner's own, or what the links before
it left. The link returns a
[`Verdict`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/points.rs):

- `Pass` keeps the answer.
- `Set(value)` changes it.
- `Refuse` stops the chain where the point allows refusals.

The heart of `haggling`, with the
definition from [the schema](#getting-a-points-definition) in `price`:

```rust
# pub mod price {
#     #[repr(transparent)]
#     #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, enshwick::Shape)]
#     #[shape(values(BUY, SELL))]
#     pub struct Deal(pub u32);
#     impl Deal {
#         pub const BUY: Self = Self(0);
#         pub const SELL: Self = Self(1);
#     }
#     #[enshwick::point(name = "you.trading-post:price", mode = Override, output = u32, refusable)]
#     #[derive(Clone, Copy, Debug)]
#     pub struct Price {
#         pub item: u32,
#         pub count: u32,
#         pub player: u64,
#         pub character: enshwick::Entity,
#         pub deal: Deal,
#     }
# }
use enshwick::points::{Order, Verdict};
use price::{Deal, Price};

const REGULARS: [u64; 1] = [0x00ab_cdef_0123_4567];
const REFUSED: [u32; 1] = [5];

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.extend_point(&Order::new(), |price: &Price, paid: &u32| {
        if price.deal != Deal::SELL {
            return Verdict::Pass;
        }
        if REFUSED.contains(&price.item) {
            return Verdict::Refuse;
        }
        if REGULARS.contains(&price.player) {
            return Verdict::Set(paid + paid / 5);
        }
        Verdict::Pass
    })
}
```

The example keeps its regulars in settings and swaps them whole when the
settings change. It does so because a link may be asked on any thread the
owner asks from. Such threads include the server's worker threads inside the
game's systems. So a link reads lock-free state, never waits and never
allocates on its common path. Each
point's documentation names its thread.

`load.observe_point(order, link)` attaches a link that only sees each answer,
for logging or statistics.

## Order

Links run one after another, each seeing the answer the ones before left, so
the last change wins.
[`Order`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/points.rs)
places a link:

```rust
use enshwick::points::Order;

let order = Order::new().after("you.haggling").before("you.bargains").priority(10);
assert_eq!(order.priority_value(), 10);
```

- `after("id")`: run after that mod's link, see its answer and have the last
  word over it.
- `before("id")`: run before that mod's link and leave it the last word.
- `priority(n)`: among links the ids leave free, the lower runs first, 0 by
  default; links with equal priority run by mod id.

Ids of mods that do not run are ignored. The loader orders the links the
same way at every start. When two links' ids contradict each other, it
breaks the cycle by priority and logs which link's order it dropped.

## Depending on the owner

Declare the owner in the [manifest](concepts/loader-and-abi.md#the-manifest),
under an alias:

```toml
[package.metadata.enshwick.after]
trading_post = "you.trading-post"
```

The alias then names the owner in the point's declaration:
`#[enshwick::point(of = trading_post, name = "price", ..)]` is
`you.trading-post:price` (see
[naming dependencies](concepts/loader-and-abi.md#the-manifest)).

With `after`, a missing owner leaves the link inert, which is no error:
the mod runs and the loader's log says
`point you.trading-post:price: the link stays inert: you.trading-post does not run`. Use
`requires` when the mod is pointless without the owner: the loader then skips
the mod with a reason. A required owner initialises first, so the loader
checks a link to its point at once. If the owner defines no such point, or
another declaration of it, `extend_point` and `observe_point` fail. Each
returns `Off`, and the log says
`point you.trading-post:price: this mod requires you.trading-post, and the link cannot match: ...`.
A reason from [the table](#when-the-loader-leaves-a-link-inert) ends the
line. Other links match once every mod is
initialised, so load order does not matter for them. In a test, the fake
loader refuses the same; define a required owner's point with
`Installed::define_point` before the mod starts. A mod links each point once. A second `extend_point` or
`observe_point` on the same point fails. It returns `Off`.

An owner that is loaded but does not export the point refuses every link to
it, required or not, and the log says
`point you.trading-post:price: you.trading-post does not export point:price; it exports ...`.
What a refused export does, in the game and in a test, is in
[What other mods may use](concepts/loader-and-abi.md#what-other-mods-may-use).

## When the loader leaves a link inert

The linking mod's log names the point and why, and the status summary lists
every point with its links and every inert link:

| Reason                                                                              | What it means                                          |
| ----------------------------------------------------------------------------------- | ------------------------------------------------------ |
| `<owner> does not run`                                                              | The owner is missing; an info line, not a warning      |
| `<owner> <version> is off`                                                          | The owner was turned off; its log says why             |
| `<owner> <version> defines no such point; update <owner>`                           | The owner is older than the declaration                |
| `<owner> <version>: ... <field> and later are newer than the owner; update <owner>` | The owner's input is older than the link's declaration |
| `<owner> <version>: field <n> is <name> here and <name> at the owner, ...`          | A field differs: the mods compiled other declarations  |
| `the answer's type, or a field, variant or value inside it, differs ...`            | The answer's type differs                              |
| `<owner> <version> declares it overridden, this mod advised; ...`                   | The mode or the refusal flag differs                   |

A reason that ends in `both mods must compile one declaration` means the two
mods were built against different versions of the point's declaration.

A link that panics is turned off with one warning,
`its link on you.trading-post:price panicked and is off; you.trading-post:price runs without it`.
The point is answered without it from then on, and the mod keeps running.

## Defining a point

The mod that owns a rule declares its point once with
[`#[point]`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/lib.rs).
The declaration gives the input struct, the name `<owner id>:<point>`, the
mode and the answer's type. When links may refuse, it adds `refusable`. Its
schema ships with the mod (see [the manifest](concepts/loader-and-abi.md#schemas-for-other-mods)), so other
mods can write their definitions from it. They can do so only when the point
is marked `export` (see [what other mods may use](concepts/loader-and-abi.md#what-other-mods-may-use)).
Without the flag, the point is the mod's own and other mods may not link it.

Every field's type and the answer's type is one of these:

- a number
- `bool`
- `enshwick::Entity` or `enshwick::content::Guid`
- a type of the mod's own with
  [`#[derive(Shape)]`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/lib.rs)

The doc comments of the point and its fields go into the schema too, each up
to its first heading. An `# Examples` section therefore stays out of other
mods' definitions, since its code compiles only in the owner. The input and
the answer have no padding, since a [Luau mod](#from-luau) reads them as
bytes. The build fails on a gap, and its bytes become fields of their own, as
for a [call](calls.md).

```rust
use std::sync::OnceLock;

use enshwick::points::OwnedPoint;

/// The price of an item at the trading post, in coins.
#[enshwick::point(name = "you.trading-post:price", mode = Override, output = u32, refusable)]
#[derive(Clone, Copy, Debug)]
pub struct Price {
    pub item: u32,
}

static PRICE: OnceLock<OwnedPoint<Price>> = OnceLock::new();

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let _first = PRICE.set(load.define_point::<Price>()?);
    Ok(())
}

fn price(item: u32) -> Option<u32> {
    let own = 10;
    let asked = PRICE.get().map(|point| point.decide(&Price { item }, own));
    match asked {
        Some(decision) if decision.is_refused() => None,
        Some(decision) => Some(decision.value_or(own)),
        None => Some(own),
    }
}
```

The mode is the owner's choice:

| Mode       | Links                                                               |
| ---------- | ------------------------------------------------------------------- |
| `Observe`  | see the answer; their changes are ignored                           |
| `Advise`   | may change the answer; the owner reads `Decision` and may ignore it |
| `Override` | may change the answer, and the owner takes it                       |

[`decide`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/points.rs)
returns one of three values:

- `Kept(answer)`
- `Changed(value)`
- `Refused`

With no live link, `decide` is one atomic load and returns at once, so a
point costs nothing until a mod extends it. With links, it is one call into the loader and one call per link,
without allocating. Ask the point where the rule is decided, on whatever
thread that is, and keep the owner's own answer cheap to compute.

Grow a point by appending fields to its input: links built against the
shorter input keep working. A changed field, a changed answer type or another
mode needs a new point name.

## Testing

With the `test` feature, the [fake loader](testing.md) plays the other side.
For a mod that owns a point, `extend_point` attaches a link as another mod
would. There, `defined_points` lists the points. For a mod that extends one,
`ask_point` asks it as the owner would, in the loader's order. There,
`attached_points` lists the links. A panicking link turns off as under the
loader. The example's test:

```rust
# pub mod price {
#     #[repr(transparent)]
#     #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, enshwick::Shape)]
#     #[shape(values(BUY, SELL))]
#     pub struct Deal(pub u32);
#     impl Deal {
#         pub const BUY: Self = Self(0);
#         pub const SELL: Self = Self(1);
#     }
#     #[enshwick::point(name = "you.trading-post:price", mode = Override, output = u32, refusable)]
#     #[derive(Clone, Copy, Debug)]
#     pub struct Price {
#         pub item: u32,
#         pub count: u32,
#         pub player: u64,
#         pub character: enshwick::Entity,
#         pub deal: Deal,
#     }
# }
use enshwick::Entity;
use enshwick::points::Decision;
use price::{Deal, Price};

fn a_regular_gets_more(loader: &enshwick::test::Installed) {
    let price = Price {
        item: 1,
        count: 1,
        player: 0x00ab_cdef_0123_4567,
        character: Entity(10),
        deal: Deal::SELL,
    };
    assert_eq!(loader.ask_point(&price, 10), Decision::Changed(12));
}
```

## Worked examples

Beside `examples/`, three released mods define points other mods may
extend:

- [`enshwick-area-loot`](https://github.com/enshwick/mods/tree/main/area-loot)
  in [`points.rs`](https://github.com/enshwick/mods/blob/main/area-loot/src/points.rs)
- [`enshwick-personal-safes`](https://github.com/enshwick/mods/tree/main/personal-safes)
  in [`points.rs`](https://github.com/enshwick/mods/blob/main/personal-safes/src/points.rs)
- [`enshwick-survivors-per-altar`](https://github.com/enshwick/mods/tree/main/survivors-per-altar)
  in [`points.rs`](https://github.com/enshwick/mods/blob/main/survivors-per-altar/src/points.rs)

Their tests link each point through the fake host.

## From Luau

A [Luau mod](luau.md) links into another mod's point as a Rust mod does. It
also [owns points](#defining-a-point-in-luau) of its own.
`cargo enshwick deps <alias> --from <owner's DLL> --luau`
writes a module for each point, call and event of a dependency.
`pack.toml` names that dependency `<alias>`. It does so in `[requires]` or
`[after]`. Each module is `deps/<alias>/<name>.luau`, with the owner's local
name. A point's module holds
the types it reaches, its named values, and each field's place with the
layout hash the owner's build wrote. The loader links only when every hash
matches the owner's. The input the module reads must also be no larger than
the owner's, and the answer must have the owner's size. A script changes an
answer only of a number, `boolean`, GUID or id type. These are the id types:

- `Entity`
- `ItemId`
- `RecipeId`
- `KnowledgeId`
- `SkillNode`
- `PlayerKey`
- `BuffTypeId`
- `NpcId`

The runtime checks the answer type's hash against the type the module names.
On another answer type, a script may observe the point. The handler gets the input and the answer so far,
and returns a new answer, `nil` to keep it, or `enshwick.refuse` on a
refusable point. An `Order` as a third argument places the link, as
`Order` does in Rust:

```luau
local price = require("./deps/trading_post/price")

enshwick.extend(price, function(input: price.Price, answer: number)
    if input.deal == price.Deal.SELL then
        return answer * 2
    end
    return nil
end, { after = { "you.haggling" } })
```

The handler runs on the thread that asks the point, under the same budget
as an event handler. An error or an answer of the wrong type turns the mod
off, and the answer stays as it was. An owner version with another layout
leaves the link idle, with the reason in the log, until the module is
written again.

### Defining a point in Luau

A Luau mod declares each point it owns in `pack.toml`, as a `[[point]]`
table. The table gives the local name, the mode, `refusable`, the input's
fields and the answer's type. A type is a number, `bool`, `enshwick.guid` or
an id type such as `enshwick.item_id`. The fields are laid out in their
order, as `#[point]` lays out a Rust struct. A field that would need padding
is refused, so list the widest first. `exports` names the point, so other
mods may link it:

```toml
id = "you.trading-post"
version = "1.0.0"
exports = ["point:price"]

[[point]]
name = "price"
mode = "override"
refusable = true
doc = "The price of an item at the trading post, in coins."
output = "u32"
input = [
    { name = "item", type = "enshwick.item_id" },
    { name = "count", type = "u32" },
]
```

`enshwick.points.define_point` defines the point at the top level of
`main.luau`. `decide` takes the input as a table and the mod's own answer.
It returns the answer with `"kept"` or `"changed"`, or `nil` with
`"refused"`:

```luau
local price = enshwick.points.define_point("price")

local function price_of(item: number): number?
    local coins, how = price:decide({ item = item, count = 1 }, 10)
    if how == "refused" then
        return nil
    end
    return coins
end
```

Other mods write their definitions from the pack's `pack.toml`, with
`cargo enshwick deps <alias> --from <pack folder>`. A Rust mod's link
matches by the layout hashes, as for a Rust owner.
