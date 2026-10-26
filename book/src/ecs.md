# ECS inside systems

Most of the game's simulation runs as ECS systems: functions that walk the rows
of a query, one row per entity that has the components they ask for.
`load.system(name)` finds a system's update function by its reflected name,
and a [hook](hooks.md) on it receives the system's query context, `ctx`. The
types in [`enshwick::ecs`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ecs.rs)
work through that context, with the engine's own functions, pinned per build in
the loader's table.

Systems run on the game's worker threads, often several at once. Use the
context only inside the hook that received it, on that thread, and follow
[Code in the game's frame](game-frame.md).

Resolve what you need once in `init` and keep it in a `OnceLock`:

```rust
use std::sync::OnceLock;

use enshwick::ecs::{Attributes, Components, Ecs, Worlds};

struct Engine {
    ecs: Ecs,
    components: Components,
    attributes: Attributes,
    worlds: Worlds,
}

static ENGINE: OnceLock<Engine> = OnceLock::new();

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let engine = Engine {
        ecs: Ecs::resolve(load)?,
        components: Components::resolve(load)?,
        attributes: Attributes::resolve(load)?,
        worlds: Worlds::resolve(load)?,
    };
    let _first = ENGINE.set(engine);
    Ok(())
}
```

Each `resolve` returns `Off` when the loader lacks one of its symbols or
constants for the running build.

## Walking the query: `Ecs`

[`Ecs`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ecs.rs)
walks the system's query with the engine's own iterator:

- `query::<N>(ctx)` starts a walk; `next()` moves to the next row and returns
  its iterator words, `&[u64; N]`, or `None` at the end. `N` is the number of
  words the system's iterator declares, its context word included. Which word
  holds which component pointer or change-tracking value depends on the system;
  find it once per system and name the indices as constants.
- `row(ctx)` is the 1-based row the query is on, 0 before the first;
  `set_row(ctx, row)` moves it. A pass of the mod's own sets it to 0 first and
  puts back the saved row afterwards, so vanilla's walk starts where it would
  have.
- `entity(ctx)` is the entity of the current row, `None` before the first.
- `mark_changed(tracker, stamp_a, stamp_b, entity, kind)` marks a written
  component changed, so replication sends it to clients. The arguments are the
  change-tracking words of the current row of a system that writes the
  component.

```rust
use enshwick::Next;
# use std::sync::OnceLock;
# static ECS: OnceLock<enshwick::ecs::Ecs> = OnceLock::new();

/// The words of this system's iterator.
const WORDS: usize = 5;

enshwick::detour! {
    static SYSTEM: unsafe extern "system" fn(ctx: *mut u64) = system;
}

fn system(next: Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    if let Some(ecs) = ECS.get() {
        // SAFETY: the system's live context, before vanilla runs.
        unsafe {
            let saved = ecs.row(ctx);
            ecs.set_row(ctx, 0);
            let mut query = ecs.query::<WORDS>(ctx);
            let mut rows = 0;
            while query.next().is_some() {
                rows += 1;
            }
            ecs.set_row(ctx, saved);
            let _ = rows;
        }
    }
    // SAFETY: the original system takes its own context.
    unsafe { next.get()(ctx) }
}
# let _ = &SYSTEM;
```

## Other entities: `Components`

[`Components`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ecs.rs)
reads what any entity holds, from inside a running system:

- `component(ctx, entity, hash)` returns a pointer to the entity's component of
  that type, or `None` when it has none. Write through it only for components
  no client sees, which need no change mark.
- `slot_root(ctx, slot, owner_index)` finds the inventory an item slot belongs
  to at the root: the chest or player holding it.

The hash names the component type. Get it at init from
`World::component_type`, by the type's reflected name:

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let world = load.world()?;
    let used_item = world.component_type(load, "keen::ecs::UsedItem")?.hash();
    enshwick::info!("UsedItem is {used_item:#010x}");
    Ok(())
}
```

[`enshwick-safes`](https://github.com/enshwick/mod-personal-safes/blob/main/src/lib.rs)
gets the `InventorySetup` type this way to find the world's Safes.

### Positions

World coordinates are 32.32 fixed point: `FIXED_PER_METRE` engine units to
the metre. `ecs::transform_position`, an `unsafe fn` over a raw component
pointer, reads a `CurrentTransform` component in metres, and `ecs::metres` and `ecs::fixed` convert between the two:

```rust
use enshwick::ecs::{FIXED_PER_METRE, fixed, metres};

let at = fixed([10.5, 2.0, -4.0]);
assert_eq!(at[0], (10.5 * FIXED_PER_METRE) as i64);
assert_eq!(metres(at), [10.5, 2.0, -4.0]);
```

## Attribute sets: `Attributes`

Components such as `Level` and `Experience` hold attributes looked up by hash
in the row's attribute registry.
[`Attributes`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ecs.rs)
reads and writes them:

- `value(registry, set, attribute)` reads one, or `None` when the set is not
  initialised or does not hold it;
- `set(registry, set, attribute, value)` writes one, and returns false when
  nothing was written.

`set` points at the component and `registry` is the attribute registry word of
the same row, both from the query's words. Mark the component changed with
`Ecs::mark_changed` when clients see it.

For experience, `Load::on_experience` does this work and hands the mod each
character's row (see [Progress](progress.md)), as
[`enshwick-shared-experience`](https://github.com/enshwick/mod-shared-experience/blob/main/src/lib.rs)
uses it.

## Which world: `Worlds`

A client that hosts a game runs two worlds, its own and the built-in server's,
and their systems share the same functions.
[`Worlds`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/ecs.rs)
tells them apart:

- `is_server(ctx)` is true for a context of the server's world: always on the
  dedicated server, on a client only for the built-in server's systems while it
  hosts a game;
- `hosting()` is true while this process runs a game server.

A server feature in a hooked system checks `is_server(ctx)` first and forwards
to `Next` otherwise. Entity ids of the two worlds are unrelated. See
[Sides and hosting](concepts/sides-and-hosting.md).
