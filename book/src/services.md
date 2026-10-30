# Services between mods

One mod can offer functions to others as a **service**, a table of
`extern "system"` functions, and tell them what happens with a **mod
event**, plain data it sends to every mod that subscribed. Mods are separate
DLLs, possibly built with different compilers months apart, so the loader
checks at load time that both sides agree on every field, and refuses a
mismatch with a message that names the field.

A service carries one mod's own logic to another. Game data, such as altars,
their zones and bases, comes from the platform's API instead (see
[Altars and bases](altars.md)), so mods need not depend on each other for it.
To let other mods change one of your mod's rules, define an extension point
instead (see [Extending other mods](extending.md)). A single function that
mods in other languages call too is a [call](calls.md).

The [examples](https://github.com/enshwick/enshwick/tree/main/examples)
folder holds the two mods this chapter follows: `trading-post` publishes item
prices and sends `you.trading-post:sold`, and `market-stalls` uses the prices and
counts the coins from each sale.

## One declaration

Declare the table and the event once, in a small crate both mods depend on,
here `trading-post-api`:

```rust
/// Item prices, published by `trading-post`.
#[enshwick::service(name = "you.trading-post:prices", version = 1)]
pub struct Prices {
    /// The price of `item` in coins, 0 for an item with no price.
    pub price: extern "system" fn(item: u32) -> u32,
    /// Sets the price of `item`; returns the old one.
    pub set_price: extern "system" fn(item: u32, price: u32) -> u32,
}

/// An item sold at the trading post.
#[enshwick::event(name = "you.trading-post:sold")]
#[derive(Clone, Copy, Debug)]
pub struct Sold {
    pub item: u32,
    pub price: u32,
}
```

- [`#[service]`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/lib.rs)
  and `#[event]` make the struct `repr(C)` and give it a layout: a hash per
  field of its name, the shape of its type, its size, alignment and offset.
- Name both after the publishing mod, as `<id>:<what>`, the form content
  ids and extension points use. An event's name must start with its
  definer's id; the loader refuses others.
- `version` is the major version of the service, 1 when left out.
- Every field's type implements [`Shape`](#shapes): its structural hash
  reaches into the type, so a change inside it refuses the link with the name
  of the outer field.
- An event's data has no padding, since a [Luau mod](luau.md#mod-events)
  hears it as bytes: the build fails on a gap, and its bytes become fields of
  their own.

Pass plain data across a table or in an event: integers, floats, `repr(C)`
structs, and pointers with a length the caller owns. An event is `Copy`.
Never pass Rust types such as `String`, `Vec` or `Box`, and never free memory
the other side allocated.

### Shapes

A field's hash reaches into its type through [`Shape`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/shape.rs).
Integers, floats, `bool` (with its two values), arrays, pointers, `Option`
and `extern` function pointers have one already, as do the API's own types
such as `Entity` and `Guid`. A type of the mod's own derives it:

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

#[enshwick::event(name = "you.market:opened")]
#[derive(Clone, Copy, Debug)]
pub struct Opened {
    pub stall: Stall,
    pub hours: Hours,
}
```

A struct hashes its name and every field with its offset and shape; an enum,
which has an integer `repr` and unit variants, every variant with its
discriminant; a newtype with `#[shape(values(..))]` the named constants with
their values. Adding a variant, renumbering one, giving a nested struct
another field or a constant another value changes the outer field's hash, so
two mods that read the same bytes differently never link.

## Publishing

[`load.publish(&TABLE)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
publishes a `'static` table during `init`:

```rust
#[enshwick::service(name = "you.trading-post:prices")]
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

## Using a service

[`load.service::<T>()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
returns the table once the loader has checked that the published table
starts with every field of `T`:

```rust
#[enshwick::service(name = "you.trading-post:prices")]
pub struct Prices {
    pub price: extern "system" fn(item: u32) -> u32,
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
`None` instead of an error, and logs a warning when a table is published but
does not serve `T`.

## Mod events

The definer defines its event during `init` and keeps the
[`Emitter`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/services.rs)
it gets back; sending calls every subscriber's handler on the sending
thread, with no queue and no allocation:

```rust
#[enshwick::event(name = "you.trading-post:sold")]
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

Other mods subscribe during `init`:

```rust
#[enshwick::event(name = "you.trading-post:sold")]
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
- The loader matches subscriptions once every mod is initialised. With no
  running mod defining the event, or a definition that lacks or changed a
  field of the subscriber's, it logs why under the subscriber's id and the
  handler stays idle.
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
major: publish the new table as `version = 2`, and keep publishing version 1
beside it while users of it remain.

## Load order

The publisher's `init` must run before the user's. Declare the publisher in
the user's manifest:

```rust
let manifest = enshwick::manifest!(
    id: "you.market-stalls",
    side: SERVER,
    requires: ["trading-post" => ">=0.1, <1.0"],
);
assert_eq!(manifest.dependency_count, 1);
```

`requires` loads the publisher first, and skips the user with a reason when
the publisher is missing or its version is out of range. Before 1.0 every
minor release may bump `0.x`, and `^0.1` refuses 0.2.0: name the whole series
that publishes the fields the user needs, such as `>=0.1, <1.0`. `after:
["id"]` orders without requiring: use it with `optional_service` or an event
the mod can do without.

## When the loader refuses

The user's log names the service and why, and `logs/enshwick/status.log`
lists every service and event with its publisher and subscribers.

| Reason                                                   | What it means                                             |
| -------------------------------------------------------- | --------------------------------------------------------- |
| `no running mod publishes it`                            | The publisher is missing, or loads later                  |
| `<id> <version> publishes it, but is off`                | The publisher was turned off; its log says why            |
| `only v2 from <id> <version> is published`               | The user was built for another major                      |
| `... fields this mod needs: <field> and later are newer` | The publisher is older than the user's declaration        |
| `field <n> is <name> here and <name> at the publisher`   | A field differs: the two mods compiled other declarations |
