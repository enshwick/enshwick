# Repository

> One Cargo workspace: the loader and its API as one project, the mods built on
> it beside them, released per crate.

## Layout

| Path                                 | Contents                                                                    |
| ------------------------------------ | --------------------------------------------------------------------------- |
| `docs/`                              | conventions, decisions, specs, plans ([documentation.md](documentation.md)) |
| `crates/enshwick-api/`               | the modding API: the ABI with the loader and the safe layer mods use        |
| `crates/enshwick-geometry/`          | altar zones as integer boxes: clusters, uniform growth, the zones service   |
| `crates/enshwick-loader/`            | the loader, `mods/enshwick/enshwick.dll`: game build, mods, hooks, log      |
| `crates/enshwick-stub/`              | the `powrprof.dll` stub that starts the loader                              |
| `crates/enshwick-reflect/`           | reads the engine's reflection data and system records from a game image     |
| `mods/enshwick-flame-altar-count/`   | the altars allowed per Flame level                                          |
| `mods/enshwick-flame-altar-zones/`   | altar clusters, uniform growth, building across seams                       |
| `mods/enshwick-furniture-comfort/`   | more comfort from furniture inside bases                                    |
| `mods/enshwick-survivors-per-altar/` | a copy of each survivor at every altar, with beds of its own altar          |
| `tools/`                             | repository scripts: the commit message check                                |
| `.github/workflows/`                 | CI and releases                                                             |

A folder appears with its first file, never ahead of it. A new crate is born
with its row in this table, its entry in the release configuration when it
ships, and, if it brings a new layer, its row in the stack table, in the same
commit.

Every crate is named `enshwick-<name>`: the loader, the API and shared code live
under `crates/`, each mod under `mods/enshwick-<id>/`, where `<id>`,
`[a-z0-9-]`, is the id in its manifest and names its settings file. Code moves
into a shared crate the first time a second crate needs it, and not before.

`research/` holds local reverse-engineering material and is ignored by git:
Keen's code and data never enter the repository.

## Stack

Versions live here and nowhere else; a decision record never carries one.

| Layer            | Choice                                                                          |
| ---------------- | ------------------------------------------------------------------------------- |
| Language         | Rust 1.98.1, edition 2024; `rust-toolchain.toml` pins it                        |
| Target           | `x86_64-pc-windows-msvc`; Linux players run it under Proton                     |
| Hooks            | MinHook through the `minhook` crate                                             |
| Tests            | cargo-nextest; divan for benchmarks; Miri on a pinned nightly for pure logic    |
| CI tools         | cargo-hack, cargo-machete, cargo-semver-checks through `taiki-e/install-action` |
| Lint, format     | clippy with `[workspace.lints]`, rustfmt (`rustfmt.toml`)                       |
| Supply chain     | cargo-deny (`deny.toml`)                                                        |
| Spelling         | typos (`_typos.toml`)                                                           |
| Repository tools | Bun 1.4: lefthook for git hooks, markdownlint-cli2, the scripts in `tools/`     |
| Releases         | release-please, per crate, with the `cargo-workspace` plugin                    |

## Checks

From the root:

| Command                                                      | Checks                                |
| ------------------------------------------------------------ | ------------------------------------- |
| `cargo fmt --all --check`                                    | layout                                |
| `cargo clippy --workspace --all-targets -- -D warnings`      | the lints of `[workspace.lints]`      |
| `cargo nextest run --workspace`                              | unit, property and integration tests  |
| `cargo test --doc --workspace`                               | doctests                              |
| `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps` | the public documentation builds clean |
| `cargo deny check`                                           | licenses, advisories, bans, sources   |
| `typos`                                                      | spelling in code and documents        |
| `bun run lint:md`                                            | markdownlint over documents           |

`bun install` installs the lefthook git hooks:

| Hook         | Runs                                                             |
| ------------ | ---------------------------------------------------------------- |
| `pre-commit` | rustfmt, markdownlint and typos on staged files                  |
| `commit-msg` | `tools/check-commit-message.ts`: the conventional form           |
| `pre-push`   | clippy, nextest, doctests and cargo-deny when Rust files changed |

CI runs the same checks on every pull request, in three jobs to keep runner
minutes low: repository checks and Linux checks on Linux, clippy and tests on
Windows, where the DLLs build. It checks each commit message, the pull request
title and the branch name. Branch protection merges only branches up to date
with their base, so a merge needs no second run. A broken `develop` or `master`
is fixed by the next pull request, before other work.

Actions in workflows are pinned by commit, with the tag in a comment.

## Tests

Unit tests sit in the module they test (`#[cfg(test)] mod tests`); tests across
a crate's public surface go in its `tests/` folder. Pure logic, such as zone
geometry or the copy bookkeeping of survivors, lives apart from the game so it
can be tested without it. Tests that need the game's binaries read their paths
from environment variables and are skipped without them. Benchmarks sit in
`benches/` with `harness = false`, written with divan; a change on a hot path
runs them before and after. Pure logic also runs under Miri in CI.

## Git

Work follows git flow, and every change reaches a shared branch through a pull
request; nothing is pushed to `develop` or `master` directly.

| Branch              | From      | Into                | Holds                                                        |
| ------------------- | --------- | ------------------- | ------------------------------------------------------------ |
| `master`            |           |                     | released code; every commit on it is released or about to be |
| `develop`           |           |                     | the next release; the default branch                         |
| `feature/<slug>`    | `develop` | `develop`           | a feature                                                    |
| `fix/<slug>`        | `develop` | `develop`           | a fix for unreleased or released code                        |
| `chore/<slug>`      | `develop` | `develop`           | docs, CI, build, refactoring, tests                          |
| `release/<version>` | `develop` | `master`, `develop` | a release: `master` merged in, then into `master`            |
| `hotfix/<slug>`     | `master`  | `master`, `develop` | an urgent fix for a release                                  |

- Both shared branches are protected for everyone, administrators included: a
  pull request with every CI check green on a branch up to date with its base,
  every review conversation resolved, a linear history, no force pushes, no
  deletion.
- Commits and tags are signed (SSH or GPG) with a key registered on GitHub.
- A pull request into `develop` is squash-merged. Its title becomes the commit,
  so it is a conventional commit, `<type>(<scope>): <subject>`, checked by CI;
  its description says what changes and how it was tested. One pull request is
  one coherent change.
- A release is a `release/<version>` branch from `develop` with `master` merged
  into it by a signed merge commit, then a pull request into `master` merged
  with a merge commit, as git flow does, so each conventional commit reaches
  release-please. `master` alone allows merge commits; `develop` stays linear
  and gets the release back by a squashed pull request.
- Commits on a branch follow the same form, checked by the `commit-msg` hook:
  the type is one of `feat`, `fix`, `perf`, `refactor`, `docs`, `test`, `build`,
  `ci`, `chore` or `revert`, the scope is the crate or mod id, or an area such
  as `conventions` or `ci`, and the subject is imperative, lower case after the
  colon, without a final period, at most 72 characters.
- `feat`, `fix`, `perf` and `revert` reach the changelog of the crate whose
  files they touch; `feat` and `fix` drive its version. A breaking change to a
  library crate carries `!` and a `BREAKING CHANGE:` footer with the migration.
- The repository holds only what a contributor or player needs: source,
  configuration, the documents of [documentation.md](documentation.md). Local
  notes, scratch files, build output and editor settings stay out.

## Releases

release-please keeps a release pull request open per changed crate on `master`,
built from conventional commits. Merging it tags the crate as
`<crate>-v<version>` and creates its GitHub release; the release workflow then
builds the DLL on Windows and attaches it. Versions follow SemVer as Cargo reads
it, so before 1.0 a breaking change bumps the minor version; cargo-semver-checks
compares the API crate with its last release. The loader's ABI version is
separate from its crate version and is described with the ABI.

`rust-version` equals the pinned toolchain and moves with it. `enshwick-api`,
the API crate, is the one crate meant for crates.io and carries its metadata and
docs.rs settings; every other crate has `publish = false`. A path dependency
carries its version.
