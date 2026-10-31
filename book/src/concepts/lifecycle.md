# The life of a mod

The loader takes every mod through five steps, all at game start.

## The five steps

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

## The mod

A mod implements
[`Mod`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs) on its
state type:

- `init(load)` registers hooks, settings, events and content, and returns the
  state or an [`Off`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs);
- `ready(&'static self, host)`, optional, runs once the hooks are armed;
- `shutdown(&'static self)`, optional, runs before a development reload
  unloads a mod with `reload = true`.

`Off::new(reason)` turns the mod off with a reason for the log. Every fallible
call on `Load` returns `Result<_, Off>`, so `?` is the usual way out.

`declare_mod!` exports the type with the manifest: `enshwick_mod_abi`,
`enshwick_mod_query`, `enshwick_mod_init`, `enshwick_mod_ready` and
`enshwick_mod_shutdown`, each catching panics. It also adds `instance()`, the
mod's state once `init` has returned it; a hook reads its settings there (see
[The mod's state in a hook](../hooks.md#the-mods-state-in-a-hook)).

```rust,standalone_crate
struct Quiet;

impl enshwick::Mod for Quiet {
    fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        Err(enshwick::Off::new("nothing to do on this build"))
    }
}

enshwick::declare_mod!(Quiet);
# assert!(Quiet::instance().is_none());
```

## When a mod fails

The loader turns the mod off and the game runs vanilla where it acted. "Off"
means its hook links are unwired from every chain, its mid-function handlers
are skipped, and its event handlers, actions and status lines no longer run.
The DLL stays loaded and the mod stays off until the next start.

| What happens | Result |
| --- | --- |
| `init` returns `Err(Off)` | off, `off: <reason>` with the mod's reason |
| `init` panics | off, `off: init panicked` |
| a hook of the mod cannot be armed | off, `off: its hook on <entry> could not be armed` |
| a hook body panics | the panic is caught, the mod is off, the call continues to the rest of the chain (see [Panics](../hooks.md#panics)) |
| an event handler, action, status closure or `ready` panics | off, `off: <what panicked>` |
| the mod calls `host.fault(reason)` | off, with that reason |
| a hardware fault in the mod's code | not caught: the game crashes as it would; `crash.log` names the mod |

A panic is caught only with `panic = "unwind"`, which the release profile of
`cargo enshwick new` keeps. A hardware fault, such as an access violation, is
not a panic. When it ends the game, the loader writes `crash.log` before the
game's own crash handling runs, naming the mod whose code faulted, whose
callback it was running, or whose code was on the stack; see
[What is in crash.log?](../faq.md#what-is-in-crashlog).

Where the reason shows, in the log, the status summary and the Mods screen, is
in [Faults](../logging.md#faults).

Other mods go on. A mod that `requires` an off mod still runs: requirements
are checked at resolve, before any `init`.
