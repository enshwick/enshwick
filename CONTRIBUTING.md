# Contributing

Enshwick is a Rust workspace. You need Rust (`rust-toolchain.toml` installs the
pinned version through rustup), [Bun](https://bun.sh) for the repository tools,
[committed](https://github.com/crate-ci/committed),
[typos](https://github.com/crate-ci/typos) and cargo-deny for the git hooks,
and on Windows the MSVC build tools.

```bash
bun install                                 # repository tools and the git hooks
cargo install --locked cargo-nextest@0.9.149
cargo nextest run --workspace
```

The hooks check formatting, spelling, Markdown and commit messages before a
commit, and again before a push, with cargo-deny when a manifest changed; none
of them compiles, so they take seconds. Before asking for review, run
`cargo xtask check`: clippy and the doctests, and the tests of the packages the
change touches and of the packages that depend on them. Off Windows it also
lints the workspace for Windows through
[cargo-xwin](https://github.com/rust-cross/cargo-xwin). The loader builds only
for Windows: off Windows, `cargo xtask check --windows` runs its tests under Wine in Docker, in an image built from
[tools/wine/Dockerfile](tools/wine/Dockerfile) on first use, or under Wine
directly on an amd64 Linux host that has it. On Apple silicon
the container runs through Rosetta, which cannot run the tests of crashes,
detours, patches and threads, so those are skipped there. `cargo xtask check
--all` runs the whole suite, the Wine tests included when Docker runs. The
rules the hooks enforce, and the ones they cannot, are in this file:

- a change to a hot path names its numbers before and after, from the
  criterion benchmarks: `cargo bench -p enshwick --features test` for hook
  dispatch and pattern scans, `-p enshwick-gamedata` for reflection and the
  archive; benchmarks that read the game set
  `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` to its executables and are skipped
  without them;

- branch from `main` as `<type>/<slug>`, `<type>` a conventional commit
  type such as `feat`, `fix` or `chore`, and open a pull request into
  `main`; its title is a conventional commit, `<type>(<scope>): <subject>`,
  and becomes the squashed commit; the merge queue merges it once CI passes;
- code follows the style of the code around it, unsafe code included;
- what mod authors need to know goes into [the modding book](book/src/SUMMARY.md)
  with the change; `bun run check:book` checks its structure, and its Rust
  examples run with `cargo test --doc --workspace`.

What a mod needs and another mod could use goes into `enshwick`, not into
the mod: events, world reads, content such as items and recipes, texts. A mod
keeps only what is its own. The API covers common needs at a high level and
always leaves the lower levels open (served resources, component reads,
detours on any function), so no mod is blocked by what the API lacks yet.
Resources several mods may change compose: their patches run in turn.

Never add game code or data: no decompiled code, no dumps, no binaries. Engine
facts go in documents as addresses and names, with the game build they were
found on.

Security issues go through [SECURITY.md](SECURITY.md), not public issues.
