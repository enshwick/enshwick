<!--
type: how-to
description: One mod offers functions to others as a service, a table of extern system functions, and tells them what happens with mod events sent to every subscribed mod.
-->

# Services between mods

One mod can offer functions to others as a **service**, a table of
`extern "system"` functions. It can also tell them what happens with a
**mod event**: plain data it sends to every mod that subscribed.

## Before you write one

Mods are separate DLLs, possibly built with different compilers months
apart. So the loader checks at load time that both sides agree on every
field. It refuses a mismatch with a message that names the field.

A service carries one mod's own logic to another. Game data, such as altars,
their zones and bases, comes from the platform's API instead (see
[Altars and bases](altars.md)). So mods need not depend on each other for it.
To let other mods change one of your mod's rules, define an extension point
instead (see [Extending other mods](extending.md)). A single function that
mods in other languages call too is a [call](calls.md).

The [examples](https://github.com/enshwick/enshwick/tree/main/examples)
folder holds the mod this chapter follows: `trading-post` publishes item
prices and sends `you.trading-post:sold`. The chapter's own examples show a
mod that uses the prices and counts the coins from each sale.

## Declaring a service and an event

The publisher declares the table and the event in its own crate, by their
local names:

```rust
/// Item prices, published by `trading-post`.
#[enshwick::service(name = "prices", version = 1)]
pub struct Prices {
    /// The price of `item` in coins, 0 for an item with no price.
    pub price: extern "system" fn(item: u32) -> u32,
    /// Sets the price of `item`; returns the old one.
    pub set_price: extern "system" fn(item: u32, price: u32) -> u32,
}

/// An item sold at the trading post.
#[enshwick::event(name = "sold")]
#[derive(Clone, Copy, Debug)]
pub struct Sold {
    pub item: u32,
    pub price: u32,
}
```

- [`#[service]`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/lib.rs)
  and `#[event]` give the struct a layout. They also make it `repr(C)`. The
  layout is a hash per field of its name, the shape of its type, its size,
  alignment and offset.
- The full name is `<id>:<what>`, with the id from the publisher's
  `Cargo.toml`. Content ids and extension points use the same form. Here it
  is `you.trading-post:prices`. An event's name must start with its definer's
  id; the loader refuses others.
- `version` is the major version of the service, 1 when left out.
- Every field's type implements [`Shape`](#shapes). Its structural hash
  reaches into the type, so a change inside it refuses the link with the name
  of the outer field.
- An event's data has no padding, since a [Luau mod](#mod-events)
  hears it as bytes. The build fails on a gap, and its bytes become fields of
  their own.

Pass plain data across a table or in an event: integers, floats, `repr(C)`
structs, and pointers with a length the caller owns. An event is `Copy`.
Never pass Rust types such as `String` or `Vec`. The same holds for `Box`.
Never free memory the other side allocated.

### Shapes

A field's hash reaches into its type through [`Shape`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/shape.rs).
These types have one already:

- integers and floats
- `bool`, with its two values
- arrays and pointers
- `Option`
- `extern` function pointers
- the API's own types, such as `Entity` and `Guid`

A type of the mod's own derives it:

```rust
use enshwick::Shape;

#[derive(Clone, Copy, Debug, Shape)]
#[repr(u8)]
pub enum Stall {
    Food = 1,
    Tools = 2,
}

#[derive(Clone, Copy, Debug, Shape)]
#[repr(transparent)]
#[shape(values(OPEN, CLOSED))]
pub struct Hours(pub u8);

impl Hours {
    pub const OPEN: Self = Self(0);
    pub const CLOSED: Self = Self(1);
}

#[enshwick::event(name = "market:opened")]
#[derive(Clone, Copy, Debug)]
pub struct Opened {
    pub stall: Stall,
    pub hours: Hours,
}
```

What a type hashes depends on its kind:

- A struct hashes its name and every field with its offset and shape.
- An enum, which has an integer `repr` and unit variants, hashes every variant
  with its discriminant.
- A newtype with `#[shape(values(..))]` hashes the named constants with their
  values.

Each of these changes the outer field's hash: adding a variant, renumbering
one, giving a nested struct another field or a constant another value. So two
mods that read the same bytes differently never link.

## Publishing

[`load.publish(&TABLE)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
publishes a `'static` table. It does so during `init`:

```rust
#[enshwick::service(name = "prices", export)]
pub struct Prices {
    pub price: extern "system" fn(item: u32) -> u32,
}

extern "system" fn price(_item: u32) -> u32 {
    10
}

static PRICES: Prices = Prices { price };

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.publish(&PRICES)
}
```

A name and major are published once: a second publisher is refused. One mod
may publish several majors of a service side by side.

Other mods reach the service and the event only when the publisher
[exports](concepts/loader-and-abi.md#what-other-mods-may-use) them. The publisher marks them
with `export` on their declarations. The declarations then read
`#[enshwick::service(name = "prices", export)]` and
`#[enshwick::event(name = "sold", export)]`. An exported table's fields are
`extern "system"` functions and data whose types and arguments a schema
describes. A schema describes numbers, `bool`, the API's own types and types that derive
`Shape`. Any other field fails the build. A Luau mod cannot call a
service, so `cargo enshwick deps --luau` leaves services out and says so.

## Using a service

[`load.service::<T>()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
returns the table once the loader has checked that the published table
starts with every field of `T`. The publisher's manifest carries the schema
of every service it exports. [`cargo enshwick deps`](extending.md#getting-a-points-definition)
writes the declaration of `T`. The file is `src/deps/trading_post.rs`, named
with the alias the user's `Cargo.toml` gives the publisher (see
[Load order](#load-order)). It holds checks that hold each field to the
publisher's layout:

```rust
#[enshwick::service(name = "prices", of = trading_post, version = 1)]
pub struct Prices {
    pub price: extern "system" fn(u32) -> u32,
}

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let prices: &'static Prices = load.service()?;
    enshwick::info!("item 3 costs {}", (prices.price)(3));
    Ok(())
}
```

No `unsafe` is needed: the check stands behind the reference. Keep it in the
mod's state and call it from any thread the publisher allows.

For a service the mod can do without, `load.optional_service::<T>()` returns
`None` instead of an error. It logs a warning when a table is published but
does not serve `T`.

## Mod events

The definer defines its event during `init` and keeps the
[`Emitter`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/services.rs)
it gets back. Sending calls every subscriber's handler on the sending
thread, with no queue and no allocation:

```rust
#[enshwick::event(name = "sold", export)]
#[derive(Clone, Copy, Debug)]
pub struct Sold {
    pub item: u32,
    pub price: u32,
}

static SOLD: std::sync::OnceLock<enshwick::services::Emitter<Sold>> =
    std::sync::OnceLock::new();

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let _first = SOLD.set(load.define_event::<Sold>()?);
    Ok(())
}

fn sell(item: u32, price: u32) {
    if let Some(sold) = SOLD.get() {
        sold.send(&Sold { item, price });
    }
}
```

The definer's manifest carries the event's schema, since the event is
declared without `of`. Other mods take the declaration
from it. [`cargo enshwick deps`](extending.md#getting-a-points-definition)
writes it into `src/deps/trading_post.rs`, with the definer's points and
calls. The declaration reads `#[enshwick::event(name = "sold", of = trading_post)]`. They
subscribe during `init`:

```rust
#[enshwick::event(name = "sold", of = trading_post)]
#[derive(Clone, Copy, Debug)]
pub struct Sold {
    pub item: u32,
    pub price: u32,
}

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.on_mod_event(|sold: &Sold| enshwick::info!("sold {} for {}", sold.item, sold.price))
}
```

- Only the definer sends its event.
- When the definer is loaded and does not export the event, the subscription
  is refused; see
  [What other mods may use](concepts/loader-and-abi.md#what-other-mods-may-use).
- The loader matches subscriptions once every mod is initialised. With no
  running mod defining the event, the loader logs why under the subscriber's
  id, and the handler stays idle. The same happens when a definition lacks or
  changed a field of the subscriber's.
- Events sent during `init` reach no one.
- A handler runs on the sender's thread, inside the sender's call: keep it
  short, and hand long work to the [scheduler](events.md).
- A panic in a handler turns the subscriber off, not the sender.

## Growing a service

Append fields at the end of the table or event, and keep the major. Users
built against the shorter declaration keep working with the newer publisher,
since the published table starts with every field they know.

A user built against a longer declaration than the publisher's is refused,
with the first field the publisher lacks:

```text
service you.trading-post:prices v1: trading-post 0.3.0: the publisher has 2 of
the 3 fields this mod needs: `refund` and later are newer than the publisher;
update trading-post
```

Changing a field already published, its type, order or meaning, is a new
major. Publish the new table as `version = 2`. Keep publishing version 1
beside it while users of it remain.

## Load order

The publisher's `init` must run before the user's. Declare the publisher in
the user's [manifest](concepts/loader-and-abi.md#the-manifest):

```toml
[package.metadata.enshwick.requires]
trading_post = { id = "you.trading-post", version = ">=0.1, <1.0" }
```

The alias names the publisher in the user's declarations.
`#[enshwick::event(of = trading_post, name = "sold")]` is
`you.trading-post:sold`. The same `of` works on `#[service]` (see
[naming dependencies](concepts/loader-and-abi.md#the-manifest)).

`requires` loads the publisher first, and skips the user with a reason when
the publisher is missing or its version is out of range. Before 1.0 every
minor release may bump `0.x`, and `^0.1` refuses 0.2.0. So name the whole series
that publishes the fields the user needs, such as `>=0.1, <1.0`. `after`
orders without requiring: use it with `optional_service` or an event
the mod can do without.

## When the loader refuses

The user's log names the service and why, and the status summary lists every
service and event with its publisher and subscribers.

| Reason                                                   | What it means                                             |
| -------------------------------------------------------- | --------------------------------------------------------- |
| `<id> does not export <name>; it exports ...`            | The publisher keeps the service or event to itself        |
| `no running mod publishes it`                            | The publisher is missing, or loads later                  |
| `<id> <version> publishes it, but is off`                | The publisher was turned off; its log says why            |
| `only v2 from <id> <version> is published`               | The user was built for another major                      |
| `... fields this mod needs: <field> and later are newer` | The publisher is older than the user's declaration        |
| `field <n> is <name> here and <name> at the publisher`   | A field differs: the two mods compiled other declarations |

## From Luau

A [Luau mod](luau.md) offers and uses another mod's work through calls and
events, not services: a service is a table of native functions.

### Other mods by alias

`pack.toml` names the mods a Luau mod depends on under aliases, as a
[data pack](content/packs.md#packtoml) does. `enshwick.deps` holds a table
per alias, as `deps!` does for a Rust mod. The aliases are those of
`requires` and `after`. In each table, `id` is the mod's id. `running()` says
whether the mod runs now. A mod of `requires`
always runs beside this one. Read `running()` for one of `after`:

```toml
[after]
lanterns = "you.lanterns"
```

```luau
if enshwick.deps.lanterns.running() then
    enshwick.log.info(`{enshwick.deps.lanterns.id} lights the way`)
end
```

### Mod events

A Luau mod hears another mod's events and sends its own.
`cargo enshwick deps <alias> --luau` writes the description of each event the
owner declares, beside its points. A description may also be written by hand. It gives each field a `name` and
how it reads. A field reads as a number, `bool`, `guid` or an id. The
integers are `u8-i64`. The floats are `f32` and `f64`. The ids are these:

- `entity`
- `item_id`
- `recipe_id`
- `knowledge_id`
- `skill_node`
- `player_key`
- `buff_type_id`
- `npc_id`

The fields are laid out and hashed as Rust lays out a `#[repr(C)]` struct of
those fields:

```luau
local greeted = enshwick.define_event({ name = "greeted", fields = {
    { name = "player", read = "entity" },
    { name = "times", read = "u16" },
} })

enshwick.on_mod_event(require("./deps/trading_post/sold"), function(sold)
    greeted.send({ player = sold.character, times = 1 })
end)
```

The loader matches a subscription by its layout hashes, as for a Rust mod.
When the defining mod's event differs or is smaller, the loader leaves the
subscription idle, with the reason in the log. `define_event` takes the
event's local name. Here `greeted` is `you.greeter:greeted`. The same holds
for `#[event(name = "greeted")]` in a Rust mod. An event a script defines has primitive fields only.
`define_event` raises an error for a new event, or for changed fields, after
the start: either takes the next start. A `send` before `main.luau` has
finished reaches no one. Other mods hear the event only when `pack.toml`
exports it, as `exports = ["event:greeted"]`.
