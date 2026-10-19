# Repository

> One Cargo workspace: the loader and its API as one project, the mods built on
> it beside them, released per crate.

## Layout

| Path                 | Contents                                                                              |
| -------------------- | ------------------------------------------------------------------------------------- |
| `docs/`              | conventions, decisions, research, specs, plans ([documentation.md](documentation.md)) |
| `tools/`             | repository scripts: commit and attribution checks                                     |
| `.github/workflows/` | CI and releases                                                                       |

A folder appears with its first file, never ahead of it. A new crate is born
with its row in this table, its entry in the release configuration and, if it
brings a new layer, its row in the stack table, in the same commit.

Crates that run or ship (the loader, each mod) live under `crates/` and
`mods/<id>/`; a mod's folder name is its id, `[a-z0-9-]`, which also names its
DLL and its config file. Code moves into a shared crate the first time a second
crate needs it, and not before.

`research/` holds local reverse-engineering material and is ignored by git:
Keen's code and data never enter the repository.

## Stack

Versions live here and nowhere else; a decision record never carries one.

| Layer            | Choice                                                                      |
| ---------------- | --------------------------------------------------------------------------- |
| Language         | Rust 1.98.1, edition 2024; `rust-toolchain.toml` pins it                    |
| Target           | `x86_64-pc-windows-msvc`; Linux players run it under Proton                 |
| Hooks            | MinHook through the `minhook` crate                                         |
| Lint, format     | clippy with `[workspace.lints]`, rustfmt (`rustfmt.toml`)                   |
| Supply chain     | cargo-deny (`deny.toml`)                                                    |
| Spelling         | typos (`_typos.toml`)                                                       |
| Repository tools | Bun 1.4: lefthook for git hooks, markdownlint-cli2, the scripts in `tools/` |
| Releases         | release-please, per crate, with the `cargo-workspace` plugin                |

## Checks

From the root:

| Command                                                 | Checks                               |
| ------------------------------------------------------- | ------------------------------------ |
| `cargo fmt --all --check`                               | layout                               |
| `cargo clippy --workspace --all-targets -- -D warnings` | the lints of `[workspace.lints]`     |
| `cargo test --workspace`                                | unit, property and integration tests |
| `cargo deny check`                                      | licenses, advisories, bans, sources  |
| `typos`                                                 | spelling in code and documents       |
| `bun run lint:md`                                       | markdownlint over documents          |

`bun install` installs the lefthook git hooks:

| Hook         | Runs                                                                   |
| ------------ | ---------------------------------------------------------------------- |
| `pre-commit` | rustfmt, markdownlint, typos and the attribution check on staged files |
| `commit-msg` | `tools/check-commit-message.ts`: conventional form, no attribution     |
| `pre-push`   | clippy, tests and cargo-deny when Rust files changed                   |

CI runs the same checks on every push and pull request, on Windows for
everything that builds the DLLs and on Linux for the rest, and checks each
pushed commit message. A broken `master` is fixed by the next commit, before
other work.

Actions in workflows are pinned by commit, with the tag in a comment.

## Tests

Unit tests sit in the module they test (`#[cfg(test)] mod tests`); tests across
a crate's public surface go in its `tests/` folder. Pure logic, such as zone
geometry or the copy bookkeeping of survivors, lives apart from the game so it
can be tested without it. Tests that need the game's binaries read their paths
from environment variables and are skipped without them.

## Git

- One branch, `master`. Commits go straight to it and are never force-pushed;
  release-please's release pull requests are the one kind of pull request.
- A commit is one coherent change, written as a conventional commit:
  `<type>(<scope>): <subject>`, where the type is one of `feat`, `fix`, `perf`,
  `refactor`, `docs`, `test`, `build`, `ci`, `chore` or `revert`, and the scope
  is the crate or mod id, or an area such as `conventions`, `ci` or `research`.
  A breaking change carries `!` after the scope. The subject is imperative,
  lower case after the colon, without a final period, at most 72 characters.
- `feat` and `fix` reach the changelog and bump the version of the crate whose
  files they touch; the others do not.
- Commits carry no tool attribution and no co-author trailer
  ([code-style.md](code-style.md#attribution)).

## Releases

release-please keeps a release pull request open per changed crate on `master`,
built from conventional commits. Merging it tags the crate as
`<crate>-v<version>` and creates its GitHub release; the release workflow then
builds the DLL on Windows and attaches it. Versions follow SemVer; the loader's
ABI version is separate from its crate version and is described with the ABI.
