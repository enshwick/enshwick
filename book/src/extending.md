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
folder holds the mod this chapter follows: `safe-viewers` extends the
[Safes](https://github.com/enshwick/mod-safes)
mod's access rule. Players listed as viewers may open every Safe, and with
`owner_removes_only` nobody but a Safe's owner removes it, administrators
included.

## The points of Enshwick's mods

The points of Enshwick's own mods are declared in
[`enshwick::points`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/points.rs),
so extending them needs no crate but the API:

| Point                             | Declaration                               | Answer | Refusal                    |
| --------------------------------- | ----------------------------------------- | ------ | -------------------------- |
| `safes:access`                    | `points::safes::Access`                   | `bool` | final: the player may not  |
| `survivors-per-altar:copy-limit`  | `points::survivors_per_altar::CopyLimit`  | `u32`  | none                       |
| `survivors-per-altar:charge-cost` | `points::survivors_per_altar::ChargeCost` | `u16`  | the staff's use is refused |
| `flame-altar-count:limit`         | `points::flame_altar_count::AltarLimit`   | `u8`   | none                       |
| `area-loot:eligible`              | `points::area_loot::Eligible`             | `bool` | none                       |

Each declaration's rustdoc says what the owner answers, on which thread the
point is asked, and what the answer means.

## Extending a point

[`load.extend_point(order, link)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/points.rs)
attaches `link` during `init`. The link gets the point's input and the answer
so far, the owner's own or what the links before it left, and returns a
[`Verdict`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/points.rs):
`Pass` keeps the answer, `Set(value)` changes it, and `Refuse` stops the
chain where the point allows refusals. The heart of `safe-viewers`:

```rust
use enshwick::points::safes::{Access, SafeAction};
use enshwick::points::{Order, Verdict};

const VIEWERS: [u64; 1] = [0x00ab_cdef_0123_4567];

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.extend_point(&Order::new(), |access: &Access, may: &bool| {
        if access.action == SafeAction::REMOVE && access.player != access.owner {
            return Verdict::Refuse;
        }
        if access.action == SafeAction::USE && !*may && VIEWERS.contains(&access.player) {
            return Verdict::Set(true);
        }
        Verdict::Pass
    })
}
```

The example keeps its viewers in settings and swaps them whole when the
settings change, since `safes:access` is asked on the server's worker threads
inside the game's systems: a link reads lock-free state, never waits and
never allocates on its common path. Each declaration names its thread.

`load.observe_point(order, link)` attaches a link that only sees each answer,
for logging or statistics.

## Order

Links run one after another, each seeing the answer the ones before left, so
the last change wins.
[`Order`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/points.rs)
places a link:

```rust
use enshwick::points::Order;

let order = Order::new().after("safe-viewers").before("safe-locks").priority(10);
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

Declare the owner in the manifest, with a version range:

```rust
let manifest = enshwick::manifest!(
    id: "you.safe-viewers",
    side: BOTH,
    recommends: ["safes" => ">=0.0.0-dev, <1.0"],
);
assert_eq!(manifest.dependency_count, 1);
```

With `recommends`, a missing owner leaves the link inert, which is no error:
the mod runs and `loader.log` says
`point safes:access: the link stays inert: safes does not run`. Use
`requires` when the mod is pointless without the owner: the loader then skips
the mod with a reason. Links match once every mod is initialised, so load
order does not matter. A mod links each point once: a second
`extend_point` or `observe_point` on the same point returns `Off`.

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
`its link on safes:access panicked and is off; safes:access runs without it`.
The point is answered without it from then on, and the mod keeps running.

## Switching a link off

A player keeps a mod but turns off one of its links in
`mods/enshwick/mods.toml`, under the linking mod's entry:

```toml
[you.safe-viewers]
enabled = true
file = "enshwick-example-safe-viewers.dll"
links_off = ["safes:access"]
```

The change takes effect at the next start.

## Defining a point

The mod that owns a rule declares its point once with
[`#[point]`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/lib.rs):
the input struct, the name `<owner id>:<point>`, the mode, the answer's type
and, when links may refuse, `refusable`. Publish the declaration in a crate
other mods can depend on, as for a service; Enshwick's own mods keep theirs in
the API.

```rust
use std::sync::OnceLock;

use enshwick::points::OwnedPoint;

/// The price of an item at the trading post, in coins.
#[enshwick::point(name = "trading-post:price", mode = Override, output = u32, refusable)]
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
use enshwick::ecs::Entity;
use enshwick::points::Decision;
use enshwick::points::safes::{Access, SafeAction};

fn viewer_opens(loader: &enshwick::test::Installed) {
    let access = Access {
        player: 0x00ab_cdef_0123_4567,
        owner: 1,
        character: Entity(10),
        safe: Entity(500),
        action: SafeAction::USE,
        admin: false,
        allowed: false,
    };
    assert_eq!(loader.ask_point(&access, false), Decision::Changed(true));
}
```
