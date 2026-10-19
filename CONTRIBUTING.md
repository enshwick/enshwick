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
and the ones they cannot, are in [docs/conventions/](docs/conventions/):

- branch from `develop` as `feature/<slug>`, `fix/<slug>` or `chore/<slug>` and
  open a pull request into `develop`; its title is a conventional commit,
  `<type>(<scope>): <subject>`, and becomes the squashed commit;
- code follows [code-style.md](docs/conventions/code-style.md), unsafe code
  included;
- nothing carries tool attribution.

Never add game code or data: no decompiled code, no dumps, no binaries. Engine
facts go in documents as addresses and names, with the game build they were
found on.

Security issues go through [SECURITY.md](SECURITY.md), not public issues.
