# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Added

- Each alpha carries `cargo-enshwick` for Windows and a static Linux build,
  with checksums, SBOM and build provenance, which
  `cargo binstall --git https://github.com/enshwick/enshwick cargo-enshwick@<version>`
  installs in seconds (#1018).
- `install` without `--game` or `ENSHROUDED_DIR` takes the folder Steam
  installed: the dedicated server, else the game, naming it and listing the
  others (#1018).
- `install --with-loader` puts in the loader of the tool's release, checked
  against the release's `SHA256SUMS`, and leaves a loader of another
  release in place with a warning (#1018).
- The tool says on stderr when a newer release of itself is out, asking
  GitHub at most once a day without waiting for the answer (#1018).

- `deps --from` reads the points a Luau mod exports from its `pack.toml`,
  its folder or the packs in a game folder's `mods/`, leaving out a pack
  whose points do not read; the Luau definitions gain
  `points.define_point`. A game field named after a Luau keyword is written
  in brackets in `enshwick.typed.d.luau`, which luau-lsp could not parse
  (#989).

## [1.0.0-alpha.13] - 2026-11-07

### Added

- `cargo enshwick luau --typed <file>` writes `enshwick.typed.d.luau`, a
  Luau type per generated item named by its path, such as
  `typed_ecs_Health`; `new --luau` writes it into `types/` and points the
  editor at it (#940).
- `cargo enshwick pin` writes a witness for each value a pattern's operand
  reads that no engine route finds: the nearest other instruction holding
  the value, in another function where one exists, or `one_witness` with
  the reason when none has a unique window (#946).

### Changed

- The crate's description names Enshwick as the modding framework for
  Enshrouded (#968).
- Breaking: `generate` and `pin` read the kept builds' tables and typed
  snapshots from a game data folder outside the repository, `--data` or
  `ENSH_GAMEDATA`; `generate` needs one, and `pin --write` in the enshwick
  repository refuses to run without one before it writes anything. The pin
  report names a kept table by its path in that folder (#980).
- `pin --old` ports an old build that has no table in `tables/` from what
  its anchors and engine routes find on it, and refreshes the check shapes
  from that; the report names where each section's old values come from,
  and an old executable without a table must keep the game's file name
  (#962).
- `pin --old --new` writes no loader table: it reports how the kept
  table's rows port, and still writes the mods' build rows and the routes'
  check shapes; `--steam-build` names the report's build and is no longer
  needed with `--write` (#951).

### Fixed

- `cargo enshwick generate` also takes a snapshot of a build no table is
  kept for: such a build is newer than every kept one, and among them the
  snapshot written last wins. `--from` snapshots a build no table is kept
  for, its side from the executable's name, and writes the typed API from
  it (#986).
- `cargo enshwick new` writes `enshwick` as a git dependency on the tag of
  the alpha the tool is from, since alphas are not on crates.io; a release
  still pins its crates.io version (#979).
- `pin --patterns` takes a witness only from a second use whose pattern
  reads the value, so a fixed pattern near the first use no longer drops
  it for `one_witness` (#951).
- `cargo enshwick pin` keeps the check of a `callers` route whose shape
  holds where the function starts, which the loader does not count as a
  check (#946).

## [1.0.0-alpha.12] - 2026-11-05

### Added

- `cargo enshwick luau` writes the `controls` namespace (#930).
- `cargo enshwick luau` writes the `items` and `rewards` namespaces;
  `pack schema` and `new --data` write `rewards.toml` and know the items'
  `on` table and setting references (#935).
- `cargo enshwick generate` writes the typed API from the newest server
  build's snapshot, snapshots an executable first with `--from`, and with
  `--check` fails when the committed files differ. `cargo enshwick pin`
  reports what the new build changed in the covered types and marks what
  needs a person; with `--write` in the enshwick repository it keeps the new
  build's snapshot and renames and writes the typed API again (#931).

### Changed

- **Breaking:** `cargo enshwick luau` writes the definitions of the whole
  API, with the Luau words for Rust's values, an `order` for
  `enshwick.extend`, and module names for types two modules share
  (`LootSource`, `InventoryOp`, `UnlockedSkill`, `ChangeField`,
  `ContentRename`) (#928).
- **Breaking:** `cargo enshwick new --side` takes `server` or `both`, a new
  Luau mod writes no `join` key, `cargo enshwick deps` reads `requires` and
  `after` alone, and `cargo enshwick manifest` shows the side instead of a
  join check line (#919).
- A new mod takes its settings with `on_settings`, from each world's
  `<stem>-mod-settings/<id>/settings.toml` (#917).
- **Breaking:** `cargo enshwick deps` reads schema format 3 and writes each
  exported service table as `#[enshwick::service(name = .., of = <alias>,
  version = ..)]` with checks that hold every field to the publisher's
  layout; `--luau` leaves services out and says why (#910).
- **Breaking:** `cargo enshwick deps` writes only the declarations an owner
  marks `export`, and reads schema format 2, whose head lists the owner's
  content and stat exports; an owner built with an older release asks for a
  rebuild, and a DLL in the folder that does not read is left out with that
  line, not the whole run (#883).
- **Breaking:** `cargo enshwick new` writes `exports` with `content:` names
  (#883).
- **Breaking:** `cargo enshwick deps` reads an owner's schemas from the
  entries its points, events and calls write into the DLL's `.enshpt`
  section, beside the head of its `declare_mod!`. An owner built with an
  older release, or a damaged section, fails with the reason and asks for
  a rebuild (#868).

### Added

- `cargo enshwick pin` refreshes the `check` shapes of the engine routes for
  the side it ports, so a hook needs no table row on the new build (#920).
- `cargo enshwick pin` takes a symbol the old table's own pattern finds once
  on the new build, and reads its code bytes again there (#914).
- `cargo enshwick new --enshwick-rev <commit>` builds the new mod on a
  commit of the platform's `main` branch from git (#696).

- The `body:<rva>` pick of `pin --patterns`: a symbol checked by its whole
  function, found through the call at `rva` (#710).

- `cargo enshwick manifest` lists what the mod exports (#783).

- `cargo enshwick luau` declares `enshwick.deps`, and says that
  `define_event` takes a local name (#788).

- `cargo enshwick deps [<alias>]` writes every point, call and mod event a
  dependency offers into `src/deps/<alias>.rs`, each declared by its local
  name with `of = <alias>`, and with `--luau` into `deps/<alias>/` (#803).

### Changed

- **Breaking:** `cargo enshwick point`, `call` and `event` are gone;
  `cargo enshwick deps` writes what they wrote (#803).

- `cargo enshwick new` declares the settings with `#[enshwick::config]`, and
  the new crate no longer depends on serde (#795).

- `cargo enshwick new` writes `exports` and `requires` into the new mod's
  `Cargo.toml` as comments, beside `recommends` and `breaks` (#792).

- **Breaking:** `cargo enshwick new` writes the mod's id, side and name into
  `[package.metadata.enshwick]` of its `Cargo.toml`, and `pin` reads and
  writes mods' build rows there, `[[package.metadata.enshwick.builds]]`,
  instead of in `manifest!` (#759).

- The `body` pick of `pin --patterns` writes a short window that finds the
  function and a hash of its whole code, never the code itself; a window
  over more than half of the function asks for `body:<rva>` (#710).
- `cargo enshwick new` writes a crate with a toolchain file, build flags,
  lints, texts and tests. Mod names are in
  title case (#696).

### Fixed

- `cargo enshwick pin` drops a constant read through a symbol it drops, with
  a note, instead of failing on the table it writes (#914).
- `cargo enshwick manifest` fails with the reason for a DLL whose
  declarations do not read, which the loader keeps off; before, it showed
  the mod as if it worked (#906).
- `Error::is_unread` tells a damaged point section or one of another format
  from other errors (#906).
- `cargo enshwick pin --report` writes the report before the tables, so a
  report that cannot be written leaves every file as it was (#694).
- `cargo enshwick install` builds a Windows DLL on every host, through
  `cargo xwin` off Windows, and names the missing target or tool (#695).
- `cargo enshwick manifest` resolves the mod's imports only from its own
  folder and the system folder, never from the current directory or `PATH`,
  and names the system's error when the mod does not load (#869).

## [1.0.0-alpha.11] - 2026-10-28

### Changed

- `enshwick.d.luau` marks the `world` namespace, `enshwick.caller` and the
  `PlayerDied` event unstable, and the Rust that `point` and `call` write
  names `enshwick::abi::SkillNode` (#643, #649).

## [1.0.0-alpha.10] - 2026-10-28

### Added

- `cargo enshwick point`, `event` and `call` know `KnowledgeId` and
  `SkillNode`, and the Luau modules they write read every id type as
  itself (`item_id`, `recipe_id`, `knowledge_id`, `skill_node`,
  `player_key`), so a script can change an answer of an id type (#619).

## [1.0.0-alpha.9] - 2026-10-27

### Added

- `cargo enshwick pin` ports a constant in a `cell` with the value read
  from the new executable at its symbol, and flags it for a human when
  that value changed (#603).

## [1.0.0-alpha.8] - 2026-10-27

### Added

- `cargo enshwick new --luau --game <game>` writes the game's own types
  beside the API's definitions, in `types/game.d.luau`, and points
  luau-lsp at them; `cargo enshwick types --luau` keeps every type clear of
  the names the API's definitions export, and
  `cargo_enshwick::luau::type_names` lists them (#583).
- `cargo enshwick call <call> --luau` writes a call's Luau module
  (`cargo_enshwick::points::luau_call`); it refuses a call whose input holds
  a value type or an enum (#577).
- `cargo enshwick call <call>` writes the Rust declaration of another mod's
  call from the server's schema (`cargo_enshwick::points::rust_call`) (#574).
- `cargo enshwick event <event>` writes a mod event's Luau module from the
  owner's schema (`cargo_enshwick::points::luau_event`) (#571).
- `cargo enshwick point --luau` writes a point's Luau module for a Luau mod
  (`cargo_enshwick::points::luau`), with each field's place in the owner's
  layout (#569).
- `cargo enshwick new --data` shows a selection of items by category in
  the commented `items.toml` (#558).
- `cargo_enshwick::luau::definitions` writes the API's Luau definitions
  (#543); `cargo enshwick luau` writes them to a file, and
  `cargo enshwick new --luau` starts a Luau mod (#545).

## [1.0.0-alpha.7] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.7, with no changes of its own.

## [1.0.0-alpha.6] - 2026-10-26

### Fixed

- `cargo enshwick point` refuses a point schema whose names, types or
  `repr` would leave their place in the generated definition, so another
  mod's DLL cannot put code into the extending mod's crate (#512).

### Added

- A library target, `cargo_enshwick::points`, reads schemas from DLLs and
  text and writes definitions, with a typed `SchemaError` (#512).

## [1.0.0-alpha.5] - 2026-10-26

### Added

- `cargo enshwick new --data` starts a data pack (#495).

## [1.0.0-alpha.4] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.4, with no changes of its own.

## [1.0.0-alpha.3] - 2026-10-26

### Added

- `cargo enshwick point <mod>:<point> --from <dll|folder|schema>` writes
  the Rust definition of another mod's extension point from the owner's DLL,
  with compile-time checks against its layout (#394).

### Changed

- `cargo enshwick new` takes an `<author>.<name>` id and names the crate
  and folder after the name (#378).
- Errors carry context: each names the file or step it was working on (#379).

## [1.0.0-alpha.2] - 2026-10-26

No changes of its own. Released with `enshwick`.

## [1.0.0-alpha.1] - 2026-10-26

The first version on crates.io (#347).
