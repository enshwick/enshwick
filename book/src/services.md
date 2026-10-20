# Services between mods

One mod can offer functions to others as a service: a `repr(C)` table of
`extern "system"` functions, published under a name and a version. Mods are
separate DLLs, possibly built with different compilers, so the table is the
whole contract between them.

A service carries one mod's own logic to another. Game data, such as altars,
their zones and bases, comes from the platform's API instead (see
[Altars and bases](altars.md)), so mods need not depend on each other for it.

## Publishing

[`load.publish(name, version, &TABLE)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs)
publishes a `'static` table during `init`:

```rust
#[repr(C)]
struct Prices {
    count: extern "system" fn() -> u32,
}

extern "system" fn count() -> u32 {
    0
}

static PRICES: Prices = Prices { count };

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    load.publish("trading-post.prices", 1, &PRICES)
}
```

- Name the service after the publishing mod, as `<id>.<what>`.
- A name and version can be published once: a second publisher of the same
  pair is refused.
- One mod may publish several versions of a service side by side, to keep
  older users working.

## Using a service

[`load.service(name, version)`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/load.rs)
returns the table during `init`:

```rust
#[repr(C)]
struct Prices {
    count: extern "system" fn() -> u32,
}

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    // SAFETY: `Prices` is the table `trading-post` publishes as v1.
    let prices: &'static Prices = unsafe { load.service("trading-post.prices", 1)? };
    enshwick_loader_api::info!("{} prices", (prices.count)());
    Ok(())
}
```

- `service` is `unsafe`: the loader cannot check the type. `T` must be exactly
  the table the publisher declared for that name and version.
- It fails with `Off` when no running mod publishes the pair. A publisher that
  is off serves nothing.
- The table is `'static`: keep the reference in the mod's state and call it
  from any thread the publisher allows.

## Load order

The publisher's `init` must run before the user's. Declare the publisher in
the user's manifest:

```rust
let manifest = enshwick_loader_api::manifest!(
    id: "market-stalls",
    side: SERVER,
    requires: ["trading-post" => ">=0.1, <1.0"],
);
assert_eq!(manifest.dependency_count, 1);
```

`requires` loads the publisher first, and skips the user with a reason when
the publisher is missing or its version is out of range. Before 1.0 every
minor release may bump `0.x`, and `^0.1` refuses 0.2.0: name the whole series
the service stays compatible with, such as `>=0.1, <1.0`, and bump the range
only when the service changes. `after: ["id"]`
orders without requiring: use it with a service the mod can do without, and
handle `load.service` failing.

## Versions

The version is a `u32` that names one table layout. Adding a function at the
end, changing an argument or changing what a function means is a new,
incompatible table: publish it as a new version. A user asks for the version
it was built against and gets that table or nothing.

The publisher's SemVer version, in `requires`, says which releases of the mod
publish which service versions; the service version says what the table looks
like.

## One declaration of the table

Put the table type, its name and its version in one file that uses nothing
but `core`, so neither mod copies the other's declaration. In one workspace,
the publisher keeps it as a module and the user compiles the same file with
`#[path = "../../<publisher>/src/service.rs"] pub mod <name>;`; across
repositories, a small crate both depend on does the same. Check the layout at
compile time there:

```rust
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Price {
    pub item: u32,
    pub buy: u32,
    pub sell: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PricesService {
    /// Writes up to `capacity` prices to `out` and returns how many items
    /// have a price, which may exceed `capacity`.
    pub prices: extern "system" fn(out: *mut Price, capacity: usize) -> usize,
}

pub const NAME: &str = "trading-post.prices";
pub const VERSION: u32 = 1;

const _: () = {
    assert!(size_of::<Price>() == 12);
    assert!(size_of::<PricesService>() == 8);
};
```

Pass plain data across the table: integers, floats, `repr(C)` structs, and
pointers with a length the caller owns, as `prices` does. Never pass Rust types
such as `String`, `Vec` or `Box`, and never free across the boundary memory the
other side allocated.
