# Contributing

Enshwick is a Rust workspace. You need Rust (`rust-toolchain.toml` installs the
pinned version through rustup), [Bun](https://bun.sh) for the repository tools,
and on Windows the MSVC build tools.

```bash
bun install                                 # repository tools and the git hooks
cargo install --locked cargo-nextest@0.9.148
cargo nextest run --workspace
```

The hooks check formatting, spelling, Markdown and commit messages before a
commit, and clippy, tests and cargo-deny before a push. The rules they enforce,
and the ones they cannot, are in this file:

- branch from `main` as `feature/<slug>`, `fix/<slug>` or `chore/<slug>` and
  open a pull request into `main`; its title is a conventional commit,
  `<type>(<scope>): <subject>`, and becomes the squashed commit;
- code follows the style of the code around it, unsafe code included;
- what mod authors need to know goes into [the modding book](book/src/SUMMARY.md)
  with the change; `bun run check:book` checks its structure, and its Rust
  examples run with `cargo test --doc --workspace`.

What a mod needs and another mod could use goes into `enshwick-loader-api`, not into
the mod: events, world reads, content such as items and recipes, texts. A mod
keeps only what is its own. The API covers common needs at a high level and
always leaves the lower levels open (served resources, component reads,
detours on any function), so no mod is blocked by what the API lacks yet.
Resources several mods may change compose: their patches run in turn.

Never add game code or data: no decompiled code, no dumps, no binaries. Engine
facts go in documents as addresses and names, with the game build they were
found on.

Security issues go through [SECURITY.md](SECURITY.md), not public issues.
