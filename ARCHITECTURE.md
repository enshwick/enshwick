# Architecture

Enshwick has four parts. A `powrprof.dll` stub starts the loader inside the
game. The loader owns everything that knows the engine. Mods talk to the loader
only through a versioned C ABI, and the `enshwick` crate wraps that ABI in a
safe API.

## Crates

| Crate                  | Where                      | Owns                                                                                                                                                                                                                  | Published               |
| ---------------------- | -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------- |
| `enshwick-loader-stub` | `crates/internal/stub`     | the `powrprof.dll` proxy: forwards the system exports, holds the dedicated server's entry point and starts the loader                                                                                                 | no, in `enshwick.zip`   |
| `enshwick-loader`      | `crates/internal/loader`   | the loader DLL: finds the game build, loads and resolves mods, links their hooks, answers the ABI                                                                                                                     | no, in `enshwick.zip`   |
| `enshwick-luau`        | `crates/internal/luau`     | Luau mods: one sandboxed VM per mod with memory and time budgets, the API bound from the script descriptions of `enshwick` and its internal parts                                                                                      | no                      |
| `enshwick`             | `crates/enshwick`          | the one crate mods use: the facade that re-exports every module of its internal parts at its own paths, the crate docs, the macros' docs and the features                                                            | yes                     |
| `enshwick-core` | `crates/internal/core` | internal part of `enshwick`: `Load` and `World`, the domains not yet in crates of their own, and the script binding, re-exported under `enshwick`'s paths | yes |
| `enshwick-base` | `crates/internal/base` | internal part of `enshwick`: what every domain shares (the host handle, events, services, points, settings, texts, state, the resource codec, the ECS reads) and the base handle `Load` holds, re-exported under `enshwick`'s paths | yes |
| `enshwick-fake` | `crates/internal/fake` | internal part of `enshwick`: the fake loader for a mod's unit tests, re-exported as `enshwick::test` behind the `test` feature | yes |
| `enshwick-macros`      | `crates/enshwick-macros`   | `#[hook]`, `#[config]` and the other macros of the API                                                                                                                                                                | yes                     |
| `enshwick-values` | `crates/internal/values` | internal part of `enshwick`: the value types its layers share (shape, content ids, resource values, the script value model), re-exported under `enshwick`'s paths | yes |
| `enshwick-abi` | `crates/internal/abi` | internal part of `enshwick`: the `repr(C)` types that cross between the loader and a mod, the ABI version and its golden layout file, and the reads and writes of game memory, re-exported as `enshwick::abi` and `enshwick::raw` | yes |
| `enshwick-typed` | `crates/internal/typed` | internal part of `enshwick`: the generated typed API, re-exported as `enshwick::typed` | yes |
| `enshwick-gamedata`    | `crates/enshwick-gamedata` | the engine routes and anchors, the compatibility reports, the engine's reflection reader (`reflect`) and the game archive reader (`archive`)                                                                          | yes                     |
| `cargo-enshwick`       | `crates/cargo-enshwick`    | the one tool for mod authors: new mods, install, manifests, content packs, point definitions, the game's types from reflection, porting a table to a new build; its library reads point schemas for tests and fuzzing | yes                     |
| `xtask`                | `tools/xtask`              | repository checks, the release archive and alpha notes, and `cargo xtask compat`, the maintainers' report of what works on a game build                                                                               | no                      |
| `enshwick-fuzz`        | `fuzz`                     | the fuzz targets for untrusted input, a workspace of their own                                                                                                                                                        | no                      |
| examples               | `examples/*`               | the whole mods and packs the book follows                                                                                                                                                                             | no                      |

## Layers

```text
stub ──starts──▶ loader ──C ABI──▶ mods (enshwick ▸ enshwick-macros)
                   │
                   ├──▶ enshwick-core, enshwick-abi
                   ├──▶ enshwick-gamedata (tables, reflect, archive) ──▶ enshwick-core
                   └──▶ enshwick-luau ──▶ enshwick-core (script descriptions)
cargo-enshwick ──▶ enshwick-core, enshwick-abi, enshwick-gamedata
enshwick ──▶ enshwick-core ──▶ enshwick-typed ──▶ enshwick-values ──▶ enshwick-macros
                 enshwick-core ──▶ enshwick-base ──▶ enshwick-abi ──▶ enshwick-values
enshwick ──test──▶ enshwick-fake ──▶ enshwick-core (its `fake` feature)
```

- `enshwick` is the one crate a mod names, a facade over its internal parts,
  each re-exported under its paths: `enshwick-core`, `Load` and the domains
  still in it; `enshwick-base`, what every domain shares; `enshwick-fake`,
  the fake loader; `enshwick-typed`, the generated
  typed API; `enshwick-abi`, the ABI; `enshwick-values`, the value types the
  layers share. None depends on `enshwick` or on a part above it, so the
  compiler refuses ABI code that names the safe API and core code that names
  the fake.
- The fake reaches inside `enshwick-core` only through its hidden `__fake`
  module, which only core's `fake` feature builds and only `enshwick-fake`
  turns on, so a mod's release build holds no test support. Core asks the
  fake about a mod's manifest through one check the fake hands down at
  install. The fake's root holds core's public items at their paths beside
  `test`: its code names them through `crate::`, and its examples name it
  `enshwick` as the other parts do.
- An internal part's doc example names its own crate `enshwick` by a hidden
  `# extern crate <crate> as enshwick;` line. One that names items above its
  crate sits in a hidden wrapper, and `crates/enshwick/src/lower_examples.rs`
  compiles its copy; `cargo xtask check` holds both.
- Only mods depend on `enshwick`. The loader, `enshwick-luau`,
  `enshwick-gamedata` and `cargo-enshwick` name the internal parts they need,
  `enshwick-core` and, for the API's version, `enshwick-abi`; they take
  `enshwick` only for their tests, which build mods and reach the fake, and
  `cargo xtask check` refuses a crate in `crates/` or `tools/` that takes it
  for more. A crate that uses the macros without the facade names the crate
  their expansion reaches with the hidden argument `crate = "path"`, as the
  loader's settings do with `crate = "::enshwick_core"`.
- Nothing depends on the loader except xtask's `compat` feature; cargo-deny
  refuses it anywhere else.
- `enshwick` never depends on the loader or on `enshwick-gamedata`: a mod links
  the API alone.
- `cargo-enshwick` shares code with the loader only through `enshwick-gamedata`
  and the ABI types.
- What a script language reaches is described once, beside the Rust items, in
  `enshwick::script`: `#[derive(Script)]` on types, `#[script]` on methods and
  the event tables. `enshwick-luau` binds those descriptions to Luau and
  `cargo-enshwick` writes the Luau definitions from them; neither keeps a
  list of its own. Only `enshwick-luau` links Luau.

## Domains

`crates/internal/core/domains.toml` orders the domains of the API, lowest
first: base, content, world, inventory, knowledge, actions, ui, packs. It
puts each top-level module still in `enshwick-core` in one domain, and names
the crate of each domain that has left it.

- A module names only its own domain and the domains before its own.
  Something two domains share moves down into base or the lower domain.
- `cargo xtask check` reads the code's `crate::` paths without compiling and
  refuses an edge up the order. The edges up the order the code still has
  are listed in `domains.toml`; a new one fails, and so does a listed one
  that is gone, so the list only shrinks.
- A domain's crate depends, among the internal crates, only on those below
  every domain (`below` in `domains.toml`) and on the crates of the domains
  before its own; the check holds its manifest, and the compiler holds its
  code.
- Registration lists are one per domain, joined in the domain order: the
  script namespaces in `script/namespaces/<domain>.rs`, which the check also
  holds to the map. The loader's `TABLE_IDS` are one list per module, joined
  in `table_ids.rs`.
- Each domain turns its own errors into `Off`, and keeps the state it needs
  while a mod initialises in the base handle's typed slots, keyed by a type
  of its own, so `Load` names no domain.
- `Load` and `World` stay in `enshwick-core`, the top layer. `Load` holds the
  base handle, `BaseLoad`, which every crate below takes for the mod's init;
  a public function below `Load` that takes the mod's loader takes
  `&impl Loading` or `&mut impl Loading`, so a mod passes its `Load` as
  before.

### Peeling a domain out of the core

Domains leave the core downward, lowest first, each in one pull request
that keeps `enshwick`'s public API equal. Base went first; each later peel
follows the same steps.

1. Make the crate `crates/internal/<domain>` (`enshwick-<domain>`): exact
   pins, the description "Internal part of enshwick; depend on enshwick",
   `#![forbid(unsafe_code)]` where it has none, `extern crate self as
   enshwick;` and a root that holds the paths the macros emit. Its
   dependencies are base, the lower domains' crates and the crates of
   `below`. Name it under `[crates]` in `domains.toml` and drop its modules
   from `[modules]`.
2. Move the domain's modules with `git mv`. Each `impl Load` block moves with
   `cargo xtask peel-load <from>=<into>`: the block becomes an `impl
   BaseLoad` block whose methods carry one hidden doc line, and its docs,
   examples and `#[script]` go to a block of bodiless methods on `Load` in
   `core/src/load/<module>.rs`, whose bodies `#[forward(base)]` writes. This
   holds only in `enshwick-base`, the crate of `BaseLoad`: Rust lets no other
   crate add inherent methods to it, so a domain above base implements its
   methods on `BaseLoad` through a hidden trait of its own crate, which the
   forwarder file imports; `peel-load` writes only the base form. A
   block of a module that `cfg` gates in its parent gets that `cfg` by
   hand. A method whose closure takes `&mut Self` stays a method of `Load`
   that runs the closure and hands the outcome down.
3. Upward edges the moved code still has go first: what a lower layer needs
   moves down with it, such as the served-resource builder into
   `resources` and the host's resource reads, and what only an upper domain
   needs moves up, such as the rate rule into `rates`. An inherent method
   of a lower type, `Host` included, lives in that type's crate.
4. The domain's `Bound` implementations and its namespace list stay in
   `core/src/script/namespaces/<domain>.rs`: a namespace opens its handle
   through `Load` and lists `Load`'s start functions, which `#[script]` on
   the forwarders makes.
5. What core or the fake reads inside the crate becomes `pub` with
   `#[doc(hidden)]`; the fake's part of it goes into the crate's own hidden
   `__fake`, behind a `fake` feature that core's turns on. Core re-exports
   the crate's modules at the paths its own code names them by, and the
   facade re-exports them at today's paths.
6. Give each doc example the alias line with `cargo xtask alias-examples`,
   run the crate's doctests without its features and with them, and feed
   each log to `cargo xtask wrap-examples <log> [--cfg <feature>]`, which
   wraps the examples that name items above the crate and writes their
   copies into `crates/enshwick/src/lower_examples/<crate>[_<feature>].rs`.
7. A per-domain cargo feature, on by default, is an optional dependency on
   the crate plus a `cfg` on core's forwarders and the facade's re-exports.

## The ABI contract

- Every type that crosses the loader and a mod is a `repr(C)` struct in
  `enshwick::abi` behind a size-and-version header.
- The loader hands each mod one `HostApi` table of `extern "system"` functions;
  a mod exports `enshwick_mod_abi`, `enshwick_mod_query`, `enshwick_mod_init`
  and `enshwick_mod_ready`, which `declare_mod!` writes.
- The ABI version is a major and a minor, the first line of the committed
  layout file `crates/internal/abi/src/abi/layout.txt`, which lists every
  field's offset, size and type, function pointers with their signature.
  An append is the next minor, any other change a new major, and the layout
  tests refuse a change without its bump. A mod of another major, or of a
  newer minor than the loader's, is refused with what to do.
- Strings are borrowed for the call, and memory is never freed across the
  boundary.
- Nothing unwinds across it: the loader catches every panic in a mod's call or
  detour and turns off that feature or that mod, never the game.

## Where engine knowledge lives

- Engine type and field names, offsets, addresses, function signatures and
  patterns live in the loader and in `enshwick-gamedata`'s anchors, never in
  `enshwick` or in a mod.
- Facts the engine reflects are found by name through `reflect` and hold across
  game builds. Entries an engine route reaches come next: the loader finds
  them in the game's code through what the engine names (its ECS systems,
  its debug strings), on every build, from the data in
  `crates/enshwick-gamedata/anchors/routes.toml`. The few no route reaches
  are pattern rows without an address in
  `crates/enshwick-gamedata/anchors/patterns.toml`, carried to every build
  first; a route that finds another address than its anchor turns the entry
  off. No file is made per build: what neither a route nor an anchor finds
  turns off, and a build where nothing is missing runs without a warning.
- The tables of the kept builds, and the reflection catalogues the typed API
  is generated from, lie outside the repository, in the game data folder
  `ENSH_GAMEDATA` names; the tests compare the routes and anchors with them
  and skip without them, and the loader does not read them. `reports/`
  holds the compatibility report of each build, a sealed test record that
  `cargo xtask check` reads. Enshwick supports the latest game version: the
  docs site and the release notes name only the newest report of each side.
- `crates/enshwick-gamedata/anchors/classes.toml` classes every table entry:
  found from the engine, or a pattern because research found no route, each
  with its reason; a test fails on an entry without one.
- In the loader, each symbol and constant it finds is declared once in
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
