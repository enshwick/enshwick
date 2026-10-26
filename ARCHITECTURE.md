# Architecture

Enshwick has four parts. A `powrprof.dll` stub starts the loader inside the
game. The loader owns everything that knows the engine. Mods talk to the loader
only through a versioned C ABI, and the `enshwick` crate wraps that ABI in a
safe API.

## Crates

| Crate                  | Where                      | Owns                                                                                                                                                                                | Published               |
| ---------------------- | -------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| `enshwick-loader-stub` | `crates/internal/stub`     | the `powrprof.dll` proxy: forwards the system exports, holds the dedicated server's entry point and starts the loader                                                               | no, ships in the bundle |
| `enshwick-loader`      | `crates/internal/loader`   | the loader DLL: finds the game build, loads and resolves mods, links their hooks, answers the ABI                                                                                   | no, ships in the bundle |
| `enshwick`             | `crates/enshwick`          | the ABI types (`enshwick::abi`), the safe API mods use, and the fake loader for tests (`test` feature)                                                                              | yes                     |
| `enshwick-macros`      | `crates/enshwick-macros`   | `#[hook]`, `#[derive(Config)]` and the other macros of the API                                                                                                                      | yes                     |
| `enshwick-gamedata`    | `crates/enshwick-gamedata` | the build tables of every game build, the engine's reflection reader (`reflect`) and the game archive reader (`archive`)                                                            | yes                     |
| `cargo-enshwick`       | `crates/cargo-enshwick`    | the one tool for mod authors: new mods, install, manifests, content packs, point definitions, porting a table to a new build; its library reads point schemas for tests and fuzzing | yes                     |
| `xtask`                | `tools/xtask`              | repository checks, the dev bundle and `cargo xtask compat`, the maintainers' report of what works on a game build                                                                   | no                      |
| `enshwick-fuzz`        | `fuzz`                     | the fuzz targets for untrusted input, a workspace of their own                                                                                                                      | no                      |
| examples               | `examples/*`               | small mods, one part of the API each                                                                                                                                                | no                      |

## Layers

```text
stub ──starts──▶ loader ──C ABI──▶ mods (enshwick ▸ enshwick-macros)
                   │
                   └──▶ enshwick-gamedata (tables, reflect, archive)
cargo-enshwick ──▶ enshwick, enshwick-gamedata
```

- The loader depends on `enshwick` for the ABI types and on `enshwick-gamedata`.
  Nothing depends on the loader except xtask's `compat` feature; cargo-deny
  refuses it anywhere else.
- `enshwick` never depends on the loader or on `enshwick-gamedata`: a mod links
  the API alone.
- `cargo-enshwick` shares code with the loader only through `enshwick-gamedata`
  and the ABI types.

## The ABI contract

- Every type that crosses the loader and a mod is a `repr(C)` struct in
  `enshwick::abi` behind a size-and-version header; structs only grow at the
  end, and a reader checks the header before it reads a newer field.
- The loader hands each mod one `HostApi` table of `extern "system"` functions;
  a mod exports `enshwick_mod_abi`, `enshwick_mod_query`, `enshwick_mod_init`
  and `enshwick_mod_ready`, which `declare_mod!` writes.
- A mod built for a newer ABI minor is refused; an older one loads.
- Strings are borrowed for the call, and memory is never freed across the
  boundary.
- Nothing unwinds across it: the loader catches every panic in a mod's call or
  detour and turns off that feature or that mod, never the game.

## Where engine knowledge lives

- Engine type and field names, offsets, addresses, function signatures and
  patterns live in the loader and in `enshwick-gamedata`'s tables, never in
  `enshwick` or in a mod.
- Facts the engine reflects are found by name through `reflect` and hold across
  game builds. The rest is in one TOML table per build under
  `crates/enshwick-gamedata/tables/`. On a build without a table, the newest
  table of the same side carries over by its patterns, and only what no pattern
  finds turns off.
- In the loader, each symbol and constant of the build table is declared once in
  `pinned/<part>.rs` as a typed `Symbol` or `Constant`, and code names it by
  path, such as `pinned::world::TIME`. Each part lists the ids it needs in its
  `TABLE_IDS`, which `cargo xtask compat` reads; a test refuses an id spelled as
  a string.
- A mod asks for a game feature through the API. The loader owns the hook on
  that feature and hands mods typed callbacks.

## Inside the loader

- `start` identifies the game and its table, loads the mods in `mods/`, calls
  their `init`, arms the hooks and calls `ready`.
- `host` answers the ABI table; each game feature has its own module
  (`inventory`, `survivors`, `weather`, …) with its hooks and state.
- `detour`, `mid` and `chains` are the hooking engine: one detour per target,
  mods linked through atomic slots, mid-function hooks with the full CPU state.
- What mods register during `init` goes into a `Registry`. The registry is
  sealed before the game runs, and hot paths read the sealed slice without a
  lock.
- Memory the loader hands the game, such as filtered lists or texts, goes
  through `retire`: it is freed only once the game's own world has stepped past
  the callers that read it.
- A large module is split by domain into `<module>/<domain>.rs`, with its tests
  in `<module>/tests.rs`, so no source file grows past about 1,000 lines.
- Modules that need no Windows API build and test on every host. The rest sits
  behind `cfg(windows)`, and its tests run on Windows or under Wine.
