### Added

- The library's `rules` and `changelog` modules hold the crate rules a mod
  workspace meets and the Keep a Changelog reader, with change files; the
  default `cli` feature holds the command and everything else, so a build
  without it is small enough for a repository's own tasks.

### Changed

- **Breaking:** `pin` ports only the build rows of the mod or workspace it
  runs in, every `Cargo.toml` below `--dir` (the current folder); `--repo`,
  `--data`, `--patterns`, `--from`, `--from-file`, `--steam-build` and
  `--game-version` are gone. Porting the loader's entries, the routes'
  check shapes and the typed API is the platform's
  `cargo xtask compat pin` and `cargo xtask compat patterns`.
