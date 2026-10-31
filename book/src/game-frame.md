# Code in the game's frame

Hooks, mid-function handlers and hooked ECS systems run inside the game's own
call stack, on its threads: the game thread, the server's game thread, and the
worker threads that run ECS systems (see [Threads](concepts/threads.md)). They
run often, many times per frame, and the frame waits for them. These rules keep
such code fast and keep the game alive.

## The common path does no heavy work

In the common case a hook does not allocate, lock, format or log.

- **No allocation.** No `Vec`, `String`, `Box` or `format!` per call. Use stack
  arrays, or a `thread_local!` value reused between calls, as
  [`enshwick-survivors-per-altar`](https://github.com/enshwick/mods/blob/main/survivors-per-altar/src/policy.rs)
  does for its plan.
- **No locks** on the fast path. A lock held by another thread stalls the
  frame; a lock the hook already holds, entered again through a nested hooked
  call, deadlocks it. Where a lock cannot be avoided, take it only after the
  fast path has decided the call is the mod's.
- **No logging per call.** Log milestones and failures. For something that may
  repeat, use `warn_once!` or `warn_every!` (see [Logging](logging.md)).

## Fast path first

Decide "not ours" in a few instructions, then forward to `Next`:

```rust
use std::sync::atomic::{AtomicBool, Ordering};

use enshwick::Next;

static ENABLED: AtomicBool = AtomicBool::new(true);

enshwick::detour! {
    static NODE_UNLOCKED: unsafe extern "system" fn(node: u32, bits: *const u64) -> u8 = node_unlocked;
}

fn node_unlocked(next: Next<'_, unsafe extern "system" fn(u32, *const u64) -> u8>, node: u32, bits: *const u64) -> u8 {
    // One compare and one relaxed load for every call that is not ours.
    if node == 7 && ENABLED.load(Ordering::Relaxed) {
        return 1;
    }
    // SAFETY: the rest of the chain takes the arguments the game passed.
    unsafe { next.call(node, bits) }
}
# let _ = &NODE_UNLOCKED;
```

## State a hook reads

State a hook reads while other threads may change it is atomic, thread-local,
or swapped whole:

- atomics for flags and numbers (`AtomicBool`, `AtomicU32`, an `f32` stored as
  bits);
- `std::sync::OnceLock` for what `init` resolves once, such as function
  pointers and offsets;
- an `ArcSwap` from the `arc-swap` crate for a table that changes as a whole,
  such as settings applied by `on_config_change`: readers load it without a
  lock, the writer stores a new one.

[`enshwick-better-altars`](https://github.com/enshwick/mods/blob/main/better-altars/src/count.rs)
keeps its altar table in an `ArcSwap`.

## Work that can wait

Work that can wait goes elsewhere: to a thread of the mod's own, started in
`Mod::ready`, or to the server's game thread through the
[scheduler](events.md). File writes in particular never happen in a hook.
[`enshwick-shared-experience`](https://github.com/enshwick/mods/blob/main/shared-experience/src/lib.rs)
writes its save on a thread it spawns from the `WorldSaved` handler.

## Unsafe code

- Every `unsafe` block names its invariant in a `// SAFETY:` comment: why the
  pointer is valid, why the type is right, why the thread is allowed.
- Raw reads and writes of game memory go through `raw::read_at` and
  `raw::write_at`, with the invariant at each call, rather than pointer casts
  spread through the code. Both copy the value unaligned and make no
  reference into game memory:

```rust
use enshwick::raw::{read_at, write_at};

let mut bytes = [1u8, 0, 0, 0, 2, 0];
// SAFETY: a `u16` lies at offset 4.
assert_eq!(unsafe { read_at::<u16>(bytes.as_ptr(), 4) }, 2);
// SAFETY: a `u16` lies at offset 4.
unsafe { write_at(bytes.as_mut_ptr(), 4, 7_u16) };
assert_eq!(bytes[4], 7);
```

- Offsets come from the loader, never from literals in the code: reflection
  through `load.field_offset`, or a pinned constant (see
  [Finding the game](finding-the-game.md)).

## The loader boundary

- Never free memory across the boundary with the loader or the game: what one
  side allocates, that side frees.
- Strings and data the loader passes are borrowed for the call. Copy what you
  keep.
- A pointer to a component or a caller frame is valid only while the game is
  where it was when the hook got it; do not keep it past the call.

## Panics

Never panic on purpose in the game's frame; on bad data, forward to vanilla.
A caught panic turns the whole mod off (see [Hooks](hooks.md#panics)).
