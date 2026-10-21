# Hooks

A hook puts a function of the mod in front of a game function. Every call the
game makes to that function goes through the mod's code first, which may change
the arguments, call on, change the result, or answer alone.

Several mods may hook one function. The loader keeps a chain of links per
function and detours the function once, at arming, after every mod is
initialised. Turning a mod off later only rewires the chain; it never touches
game code again.

## Declaring a hook

[`detour!`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/hook.rs)
declares a static
[`Hook`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/hook.rs):
the mod's link, with an entry point that has the game function's signature and
catches panics. The body is an ordinary function that takes a
[`Next`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/hook.rs)
and the target's arguments:

```rust
use enshwick_loader_api::Next;

type MaxAltars = unsafe extern "system" fn(balancing: u64, level: u32) -> u8;

enshwick_loader_api::detour! {
    static MAX_ALTARS: unsafe extern "system" fn(balancing: u64, level: u32) -> u8 = max_altars;
}

fn max_altars(next: Next<MaxAltars>, balancing: u64, level: u32) -> u8 {
    // SAFETY: the rest of the chain takes the arguments the game passed.
    let vanilla = unsafe { next.get()(balancing, level) };
    vanilla.max(16)
}

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let max_altars = load.symbol("building.max_altars")?;
    // SAFETY: the game's altar limit has the type `MAX_ALTARS` declares.
    unsafe { load.hook(&MAX_ALTARS, max_altars) }
}
```

- The signature is an `extern "system" fn` or `unsafe extern "system" fn` with
  up to twelve arguments, named. It must match the game function exactly: the
  loader cannot check it.
- `next.get()` returns the next link, or the game's original when this link is
  the last. Call it to continue; skip it to replace the function. A call
  through `Next` costs one atomic load and one indirect call. `Next` is `Copy`.
- `load.hook(&HOOK, address)` puts the link into the chain of the function at
  `address` (an absolute address from [Finding the game](finding-the-game.md)).
  Hooks are linked during `init` only; nothing runs until arming. The call is
  `unsafe`: its `SAFETY` comment says why the function at `address` has the
  hook's type, which the loader cannot check.

[`#[hook]`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/lib.rs)
writes the same declaration from the body. The static is named after the
function in upper case:

```rust
use enshwick_loader_api::{Next, hook};

#[hook]
fn upgrade(next: Next<unsafe extern "system" fn(*mut u64)>, ctx: *mut u64) {
    // SAFETY: the original takes the same context.
    unsafe { next.get()(ctx) }
}

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    let target = load.system("upgrade_building_range")?;
    // SAFETY: the system takes the building context, as `UPGRADE` does.
    unsafe { load.hook(&UPGRADE, target) }
}
```

## Chains and priority

The links on one function run by priority, lower first. Links with equal
priority run in load order: dependencies first, then by mod id. The default
priority is 0.

```rust
use enshwick_loader_api::{Next, abi::HookFlags};

enshwick_loader_api::detour! {
    static LOOKUP: unsafe extern "system" fn(bases: *mut u8, npc: *const u32) -> u32 =
        lookup, priority = -10;
}

fn lookup(next: Next<unsafe extern "system" fn(*mut u8, *const u32) -> u32>, bases: *mut u8, npc: *const u32) -> u32 {
    // SAFETY: forwarded unchanged.
    unsafe { next.get()(bases, npc) }
}
# let _ = (&LOOKUP, HookFlags::NONE);
```

With `#[hook]`, the same settings go in the attribute:
`#[hook(priority = -10, flags = HookFlags::CALLER_FRAME)]`.

`load.hook` refuses, and returns `Off`:

- a second link of the same mod on one function;
- any link on a function another mod holds with `HookFlags::EXCLUSIVE`, and an
  exclusive link on a function another mod already links;
- a link asked for after `init`.

At arming, a function the loader cannot detour (its first instructions do not
decode or cannot move, or it is shorter than the 5-byte jump) stays unhooked,
and every mod linked on it is turned off.

## Flags

[`HookFlags`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs)
has two flags, combined with `HookFlags(a.0 | b.0)`:

- `EXCLUSIVE`: no other mod may link on this function. Use it for a link that
  replaces the function and cannot share it. The loader
  [links](https://github.com/themaiby/enshwick/blob/main/crates/internal/loader/src/altars.rs)
  `upgrade_building_range` so while a mod controls altar zones.
- `CALLER_FRAME`: the loader records the game caller's stack pointer and
  callee-saved registers at entry of the chain. The link reads them with
  `host().caller_frame()`.

## Telling callers apart

A function that many places call often needs a change for one of them only.
With `CALLER_FRAME`, the
[`CallerFrame`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs)
names the call site: `return_address()` reads the address the function returns
to, which a mod pins per build in its own rows. `stack()` is the caller's stack
above the return address, and the callee-saved registers are as the caller had
them, for reading its locals.

```rust
use std::sync::atomic::{AtomicU64, Ordering};

use enshwick_loader_api::{Next, abi::HookFlags};

type NodeUnlocked = unsafe extern "system" fn(node: u32, bits: *const u64) -> u8;

/// The return address of the one call site to change; set in `init`.
static GATE: AtomicU64 = AtomicU64::new(0);

enshwick_loader_api::detour! {
    static NODE_UNLOCKED: unsafe extern "system" fn(node: u32, bits: *const u64) -> u8 =
        node_unlocked, flags = HookFlags::CALLER_FRAME;
}

fn node_unlocked(next: Next<NodeUnlocked>, node: u32, bits: *const u64) -> u8 {
    if node == 7 && let Some(host) = enshwick_loader_api::host() {
        // SAFETY: the frame was taken at entry of this chain.
        let from = unsafe { host.caller_frame().return_address() };
        if from == GATE.load(Ordering::Relaxed) {
            return 1;
        }
    }
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { next.get()(node, bits) }
}

fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    GATE.store(load.symbol("my-mod.gate_return")?, Ordering::Relaxed);
    let node_unlocked = load.symbol("skill.node_unlocked")?;
    // SAFETY: the game's node unlock has the type `NODE_UNLOCKED` declares.
    unsafe { load.hook(&NODE_UNLOCKED, node_unlocked) }
}
```

Read the frame before calling anything hooked: it is valid until the link calls
a hooked function.

## The mod's state in a hook

`declare_mod!` adds `instance()` to the mod's type: `Some(&'static Self)` once
`init` has returned the state, `None` before. A hook can read settings there:

```rust
use enshwick_loader_api::{Load, Mod, Next, Off};

struct MyMod {
    altars: u8,
}

enshwick_loader_api::detour! {
    static MAX_ALTARS: unsafe extern "system" fn(balancing: u64, level: u32) -> u8 = max_altars;
}

fn max_altars(next: Next<unsafe extern "system" fn(u64, u32) -> u8>, balancing: u64, level: u32) -> u8 {
    match MyMod::instance() {
        Some(state) => state.altars,
        // SAFETY: the original takes the arguments the game passed.
        None => unsafe { next.get()(balancing, level) },
    }
}

impl Mod for MyMod {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(id: "my-mod", side: BOTH, join = true)
    }

    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let max_altars = load.symbol("building.max_altars")?;
        // SAFETY: the game's altar limit has the type `MAX_ALTARS` declares.
        unsafe { load.hook(&MAX_ALTARS, max_altars) }?;
        Ok(Self { altars: 16 })
    }
}

enshwick_loader_api::declare_mod!(MyMod);
# fn main() {}
```

State that changes while the game runs, such as settings applied by
`on_config_change`, goes in atomics or an `ArcSwap` instead; see
[Code in the game's frame](game-frame.md).

## Panics

The entry point `detour!` writes catches a panic in the body. The mod then
reports a fault, the loader turns it off and rewires every chain past all of
its links, and the call that panicked continues to the rest of the chain with
the arguments the game passed. The game runs vanilla where the mod was and
never crashes for it.

The entry point cannot tell whether the body called `next` before the panic,
so a panic after `next` runs the rest of the chain, and the game's function, a
second time. Work after `next` goes in `Hook::after_next`: a panic there turns
the mod off the same way, and the detour returns what `next` gave. The guard
costs nothing until a panic.

```rust
# use std::sync::atomic::{AtomicU32, Ordering};
# static RAISED: AtomicU32 = AtomicU32::new(0);
enshwick_loader_api::detour! {
    static RAISE: extern "system" fn(level: u32) -> u32 = raise;
}

fn raise(next: enshwick_loader_api::Next<extern "system" fn(u32) -> u32>, level: u32) -> u32 {
    let raised = next.get()(level);
    let _counted = RAISE.after_next(|| RAISED.fetch_add(raised - level, Ordering::Relaxed));
    raised
}
# fn main() {}
```

A panic is a last resort, not error handling. A hook that can fail returns to
vanilla on its own, as
[`enshwick-area-loot`](https://github.com/themaiby/enshwick/blob/main/mods/enshwick-area-loot/src/take.rs)
does with its `OFF` flag.

## One function, both halves of a mod

A client that hosts a game runs the server's code, so a `BOTH` mod may link its
server half and its client half in one process. When both halves need the same
function, the mod still links it once: the loader refuses a second link of one
mod on one function. Link one detour that answers for both, and skip the
other half's link.

Some functions serve both worlds of a hosting client, the client's and the
built-in server's. A hook on one of them that should act for the server only
asks [`ecs::Worlds`](ecs.md#which-world-worlds) first.

## Testing

[The test fake](testing.md) runs a mod's hooks without the game: a fake
loader whose chain ends at a plain function standing in for the game's.
