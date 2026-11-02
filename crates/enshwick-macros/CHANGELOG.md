# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Changed

- **Breaking:** `declare_mod!` and `declare_test_mod!` take no `points`,
  `events` or `calls` lists and no longer check them against `exports`.
  `#[point]`, `#[event]` and `#[call]` without `of` write their schemas
  through `__schema_entry!`, and `__declare_exports!` takes a
  `__private::Manifested` (#868).

- **Breaking:** `texts!()` refuses an empty text in any language. It takes
  every language rule from `Language::REQUIRED` and names the languages a
  key lacks. Its error for different values lists each language's values.
  A key's constant doc shows the key's text in every language (#839).

- **Breaking:** `texts!()` and `declare_mod!` refuse a texts key without a
  text in each language of `Language::REQUIRED`, `en` and `uk` (#838).

- **Breaking:** `declare_mod!` embeds and checks the crate's
  `src/texts.toml` and hands it to `__declare_exports!`, which takes it as
  a third argument; `test::declare_mod!` takes `texts = "<path>"` (#827).

- **Breaking:** a required dependency's `deps::<alias>::id` from `deps!` is
  no longer `const`: it asks the loader and returns `Result<Id, Off>`, `Off`
  for a name the dependency does not export. An optional one's returns
  `None` for such a name (#798).

### Added

- `texts!()`, a constant per key of the crate's `src/texts.toml`, checked
  when the crate builds (#827).

- `#[config]`, which declares a settings struct: serde's derives through the
  API's serde, unknown keys refused and `Config` (#795).

- **Breaking:** `#[derive(Config)]` refuses a struct without
  `#[serde(deny_unknown_fields)]` (#795).

- `exports` in `[package.metadata.enshwick]`, the local names other mods may
  use, which `declare_mod!` puts into the manifest; a listed point, event or
  call missing from it fails the build (#783).

- `declare_mod!` and `declare_test_mod!`, which build a mod's manifest from
  its `[package.metadata.enshwick]` table (#759).

- `id!`, the content id `<mod>:<name>` of a local name, with the mod id
  from the same table (#766).

- `deps!`, a module per dependency alias of the same table, and `of = <alias>`
  with local names on `#[call]`, `#[point]`, `#[service]` and `#[event]`
  (#773).

- `deps!` also writes `deps::vanilla`, the running game's content by its
  debug names; the table refuses a dependency with the alias `vanilla`
  (#780).

## [1.0.0-alpha.11] - 2026-10-28

### Changed

- Released beside `enshwick` 1.0.0-alpha.11, which pins it exactly; no
  changes of its own.

## [1.0.0-alpha.10] - 2026-10-28

### Changed

- Released beside `enshwick` 1.0.0-alpha.10, which pins it exactly; no
  changes of its own.

## [1.0.0-alpha.9] - 2026-10-27

### Changed

- Released beside `enshwick` 1.0.0-alpha.9, which pins it exactly; no
  changes of its own.

## [1.0.0-alpha.8] - 2026-10-27

### Changed

- `#[point]` and `#[event]` fail the build on padding in a point's input or
  answer or an event's data, with the message `#[call]` gives (#587).

### Fixed

- A `#[script]` method whose closure parameter lends its argument (`&T`)
  hands the script a copy instead of failing to compile (#581).

## [1.0.0-alpha.7] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.7, with no changes of its own.

## [1.0.0-alpha.6] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.6, with no changes of its own.

## [1.0.0-alpha.5] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.5, with no changes of its own.

## [1.0.0-alpha.4] - 2026-10-26

Released beside `enshwick` 1.0.0-alpha.4, with no changes of its own.

## [1.0.0-alpha.3] - 2026-10-26

### Added

- `manifest!` takes `was: [...]` for a renamed mod (#369) and
  `points: [...]` for the extension points a mod defines (#394), and checks
  that a mod id has an author part (#378).
- `#[point]` writes `Point::SCHEMA`, and `#[derive(Shape)]` writes the
  schema names and types with their doc comments; `#[point]` refuses a type
  a schema cannot describe (#394).
- `#[config(group)]` for a table of settings (#399) and `#[config(first = n)]`
  for the number of a list's first element (#425).

## [1.0.0-alpha.2] - 2026-10-26

### Removed

- Chat command generation for actions (#336).

## [1.0.0-alpha.1] - 2026-10-26

The first version on crates.io (#347).
