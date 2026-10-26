# The life of a mod

The loader takes every mod through five steps, all at game start.

1. **Query.** The loader calls `enshwick_mod_query` and reads the manifest:
   id, version, side, dependencies, pinned build rows. No other code of the
   mod runs yet. A malformed manifest skips the mod.
2. **Resolve.** The loader skips, each with a reason in `loader.log`:
   - mods for the other side (a dedicated server skips `CLIENT` mods);
   - a second mod with an id already taken, in file name order;
   - mods whose `requires` is missing, skipped or out of range, as in
     `requires trading-post ^0.2, found 0.1.3`;
   - mods in a dependency cycle.

   The rest load with their dependencies first, otherwise in id order.
3. **Init.** `Mod::init` runs once per mod, in load order, on the loader's
   start thread. It links hooks, reads settings, subscribes to events, finds
   game functions and services, and adds content. Nothing takes effect yet:
   hooks are only recorded. Hooks and event subscriptions
   are refused after `init`.
4. **Arm.** Once every mod is initialised, the loader writes one detour per
   hooked function and wires each chain past the mods that are off. On the
   dedicated server the server's own code has not started yet: it waits at
   its entry point until the hooks are armed, at most 10 seconds, so a hook
   sees even the server's start-up, such as its task system. On the client,
   whose start-up runs under Steam's DRM wrapper, the loader instead waits
   for the game's window, at most 15 seconds, and the game's start-up has
   run by then.
5. **Ready.** `Mod::ready` runs for every mod still on, in load order. Start
   background threads here.

## When a mod fails

The loader turns the mod off and the game runs vanilla where it acted. "Off"
means its hook links are unwired from every chain, its mid-function handlers
are skipped, and its event handlers no longer run. The DLL stays loaded and
the mod stays off until the next start. `loader.log` and the mod's log say
why.

| What happens | Result |
| --- | --- |
| `init` returns `Err(Off)` | off, the reason in the log |
| `init` panics | off, "init panicked" |
| a hook of the mod cannot be armed | off |
| a hook body panics | the panic is caught, the mod is off, the call continues to the rest of the chain |
| an event handler or `ready` panics | off |
| the mod calls `host.fault(reason)` | off, with that reason |
| a hardware fault in the mod's code | not caught: the game crashes as it would; `crash.log` names the mod |

A panic is caught only with `panic = "unwind"`, which the release profile of
`cargo enshwick new` keeps.

Other mods go on. A mod that `requires` an off mod still runs: requirements
are checked at resolve, before any `init`.

## `Mod::instance()`

`declare_mod!` adds `instance()` to the mod's type: `Some(&state)` once `init`
has returned it, `None` before and when `init` failed. A hook that runs
before the state exists, or whose mod failed at init, forwards to `next`:

```rust,standalone_crate
# struct MyMod { altars: u8 }
# impl enshwick::Mod for MyMod {
#     fn manifest() -> &'static enshwick::abi::Manifest {
#         enshwick::manifest!(id: "you.my-mod", side: SERVER)
#     }
#     fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
#         Ok(Self { altars: 16 })
#     }
# }
# enshwick::declare_mod!(MyMod);
use enshwick::Next;

fn max_altars(next: Next<unsafe extern "system" fn(u64, u32) -> u8>, balancing: u64, level: u32) -> u8 {
    match MyMod::instance() {
        Some(state) => state.altars,
        // SAFETY: the original takes the arguments the game passed.
        None => unsafe { next.get()(balancing, level) },
    }
}
```

`instance()` stays `Some` after a later fault; the loader stops calling the
mod's hooks and handlers instead.
