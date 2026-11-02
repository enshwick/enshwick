# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Added

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
