# Extending other mods

A mod can let other mods change one of its rules at an **extension point**:
a typed question the mod asks whenever the rule applies, with its own answer
first and every other mod's link after it. Links attach in order, may change
the answer or refuse where the point allows, and are checked against the
point's declaration field by field when the game starts, like
[services](services.md). A link that panics is turned off alone, the player
can switch one link off without the mod, and the Mods screen and the
[conflict report](logging.md#conflicts) show who extends what.

The [examples](https://github.com/enshwick/enshwick/tree/main/examples)
folder holds the mods this chapter follows: `trading-post` defines the point
`you.trading-post:price`, what it pays for an item, and `haggling` extends it.
Regular customers get more for what they sell, and the trading post buys no
item on the refused list.

## Getting a point's definition

A mod that extends a point compiles a definition of it: the input struct, the
answer's type and every type they reach. The owner's build writes a schema of
each point it lists in its manifest, and `cargo enshwick point` writes the
definition from it:

```bash
cargo enshwick point you.trading-post:price --from <game folder> --out src/price.rs
```

`--from` is the owner's DLL, a folder with it in it or in its `mods/`, such as
the game's, or a schema file the owner published. The tool reads the schema
from the DLL without running it. Declare the written module `pub` (`pub mod
price;`), so its types count as the mod's own API. The file ends with checks
that hold it to the owner's layout; write it again when the owner adds fields
or points, rather than editing it. A definition older than the owner keeps
working, since points grow by appending fields, and one that no longer matches
the owner leaves the link inert with the reason in the log.

### The schema

The schema is TOML, the same for every language a definition is written in:

| Table                                | Keys                                                                                                               |
| ------------------------------------ | ------------------------------------------------------------------------------------------------------------------ |
| top                                  | `schema` (the format, 1), `mod` (the owner's id), `version` (the owner's)                                          |
| `[[point]]`                          | `name`, `mode` (`observe`, `advise`, `override`), `refusable`, `input`, `output`, `doc`, `layout`, `output_layout` |
| `[[type]]`                           | `name`, `doc`, `kind` (`struct`, `newtype`, `values`, `enum`), `repr` for `values` and `enum`                      |
| `[[type.field]]`                     | `name`, `doc`, `type`                                                                                              |
| `[[type.value]]`, `[[type.variant]]` | `name`, `doc`, `value`                                                                                             |

A type is a number (`u8` to `u64`, `i8` to `i64`, `f32`, `f64`), `bool`,
`enshwick.entity`, `enshwick.guid`, or a `[[type]]` of the schema. `layout`
lists each input field's layout hash and `output_layout` the answer's, as
decimal text: a definition checks itself against them.

## Extending a point

[`load.extend_point(order, link)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/points.rs)
attaches `link` during `init`. The link gets the point's input and the answer
so far, the owner's own or what the links before it left, and returns a
[`Verdict`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/points.rs):
`Pass` keeps the answer, `Set(value)` changes it, and `Refuse` stops the
chain where the point allows refusals. The heart of `haggling`, with the
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
settings change: a link may be asked on any thread the owner asks from, such
as the server's worker threads inside the game's systems, so it reads
lock-free state, never waits and never allocates on its common path. Each
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
under an alias, with a version range or none:

```toml
[package.metadata.enshwick.recommends]
trading_post = "you.trading-post"
```

The alias then names the owner in the point's declaration:
`#[enshwick::point(of = trading_post, name = "price", ..)]` is
`you.trading-post:price` (see
[naming dependencies](concepts/loader-and-abi.md#the-manifest)).

With `recommends`, a missing owner leaves the link inert, which is no error:
the mod runs and `loader.log` says
`point you.trading-post:price: the link stays inert: you.trading-post does not run`. Use
`requires` when the mod is pointless without the owner: the loader then skips
the mod with a reason. A required owner initialises first, so the loader
checks a link to its point at once: when the owner defines no such point, or
another declaration of it, `extend_point` and `observe_point` return `Off`
and the log says
`point you.trading-post:price: this mod requires you.trading-post, and the link cannot match: ...`
with a reason of the table below. Other links match once every mod is
initialised, so load order does not matter for them. In a test, the fake
loader refuses the same; define a required owner's point with
`Installed::define_point` before the mod starts. A mod links each point once: a second
`extend_point` or `observe_point` on the same point returns `Off`.

An owner that is loaded but does not export the point refuses every link to
it, required or not, and the log says
`point you.trading-post:price: you.trading-post does not export price; it exports ...`.
What a refused export does, in the game and in a test, is in
[What other mods may use](concepts/loader-and-abi.md#what-other-mods-may-use).

## When the loader leaves a link inert

The linking mod's log names the point and why, and `status.log` lists every
point with its links and every inert link:

| Reason                                                                              | What it means                                          |
| ----------------------------------------------------------------------------------- | ------------------------------------------------------ |
| `<owner> does not run`                                                              | The owner is missing; an info line, not a warning      |
| `<owner> <version> is off`                                                          | The owner was turned off; its log says why             |
| `<owner> <version> defines no such point; update <owner>`                           | The owner is older than the declaration                |
| `<owner> <version>: ... <field> and later are newer than the owner; update <owner>` | The owner's input is older than the link's declaration |
| `<owner> <version>: field <n> is <name> here and <name> at the owner, ...`          | A field differs: the mods compiled other declarations  |
| `the answer's type, or a field, variant or value inside it, differs ...`            | The answer's type differs                              |
| `<owner> <version> declares it overridden, this mod advised; ...`                   | The mode or the refusal flag differs                   |
| `off in mods/enshwick/mods.toml`                                                    | The player switched the link off                       |

A reason that ends in `both mods must compile one declaration` means the two
mods were built against different versions of the point's declaration.

A link that panics is turned off with one warning,
`its link on you.trading-post:price panicked and is off; you.trading-post:price runs without it`.
The point is answered without it from then on, and the mod keeps running.

## Switching a link off

A player keeps a mod but turns off one of its links in
`mods/enshwick/mods.toml`, under the linking mod's entry:

```toml
[you.haggling]
enabled = true
file = "enshwick_example_haggling.dll"
links_off = ["you.trading-post:price"]
```

The change takes effect at the next start.

## Defining a point

The mod that owns a rule declares its point once with
[`#[point]`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/lib.rs):
the input struct, the name `<owner id>:<point>`, the mode, the answer's type
and, when links may refuse, `refusable`. List it under `points` in
`declare_mod!` (see [the manifest](concepts/loader-and-abi.md#the-manifest)),
as `enshwick::declare_mod!(TradingPost, points: [Price])`, so its schema ships with
the mod and other mods can write their definitions from it, and add its
local name to `exports` in the manifest, `exports = ["price"]`, so other mods
may link it; a listed point missing from `exports` fails the build. Every field's
type and the answer's are numbers, `bool`, `enshwick::Entity`,
`enshwick::content::Guid`, or types of the mod's own with
[`#[derive(Shape)]`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/lib.rs);
the doc comments of the point and its fields go into the schema too, each up
to its first heading, so an `# Examples` section, whose code compiles only in
the owner, stays out of other mods' definitions. The input and the answer have
no padding, since a [Luau mod](luau.md#extending-another-mod) reads them as
bytes: the build fails on a gap, and its bytes become fields of their own, as
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
returns `Kept(answer)`, `Changed(value)` or `Refused`. With no live link it
is one atomic load and returns at once, so a point costs nothing until a mod
extends it; with links it is one call into the loader and one call per link,
without allocating. Ask the point where the rule is decided, on whatever
thread that is, and keep the owner's own answer cheap to compute.

Grow a point by appending fields to its input: links built against the
shorter input keep working. A changed field, a changed answer type or another
mode needs a new point name.

## Testing

With the `test` feature the [fake loader](testing.md) plays the other side:
for a mod that owns a point, `extend_point` attaches a link as another mod
would and `defined_points` lists the points; for a mod that extends one,
`ask_point` asks it as the owner would, in the loader's order, and
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
