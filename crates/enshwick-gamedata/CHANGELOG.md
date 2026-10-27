# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Added

- `Image::relocations` lists the 64-bit addresses Windows moves when it
  maps an image away from its preferred base (#555).

## [1.0.0-alpha.7] - 2026-10-26

### Added

- The catalogue names an item's flags and `ItemCategory`, which selections
  of items read, and every member of `keen::ItemCategory` (#540).
- The tables of server 399 and client 426 pin `voxel.read_box` and
  `voxel.store` (#541).

## [1.0.0-alpha.6] - 2026-10-26

### Added

- The catalogue names the loot table types and fields that data pack loot
  changes resolve (#523).

## [1.0.0-alpha.5] - 2026-10-26

### Added

- Reads the constants of reflected enums by name, and the base type of a
  reflected struct, so the loader resolves recipe kinds, event types, damage
  types and game settings values without numbers in code (#461, #469, #494).
- Catalogue facts for game events, recipe kinds, world, inventory, loot,
  stats and game settings (#461, #465, #469, #494).

## [1.0.0-alpha.4] - 2026-10-26

### Added

- An engine catalogue that the loader and `cargo enshwick` share, resolving
  engine types, sizes and field offsets by enshwick ids from the game's
  reflection (#460).

## [1.0.0-alpha.3] - 2026-10-26

### Added

- `enshwick_gamedata::reflect`, the reflection reader that was the
  `enshwick-reflect` crate (#421).
- `Resolved::parts` tells, for each part of a program, which of its
  build table entries a game build lacks (#410).

### Fixed

- A search through a chunk goes on past an entry that panics (#396).

## [1.0.0-alpha.2] - 2026-10-26

No changes of its own. Released with `enshwick`.

## [1.0.0-alpha.1] - 2026-10-26

The first version on crates.io, with the build tables and the game-archive
reader (#347).
