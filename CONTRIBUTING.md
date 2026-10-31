# Contributing

Enshwick is a Rust workspace. You need Rust (`rust-toolchain.toml` installs the
pinned version through rustup), [lefthook](https://lefthook.dev),
[rumdl](https://github.com/rvben/rumdl),
[typos](https://github.com/crate-ci/typos), cargo-deny,
[actionlint](https://github.com/rhysd/actionlint) and
[zizmor](https://docs.zizmor.sh) for the git hooks,
[ast-grep](https://ast-grep.github.io) and
[cargo-machete](https://github.com/bnjbvr/cargo-machete) for
`cargo xtask check`, and on Windows the MSVC build tools.

```bash
lefthook install                            # the git hooks
rumdl check .                               # every document, as the hooks check it
cargo install --locked cargo-nextest@0.9.150
cargo nextest run --workspace
```

The hooks check formatting, spelling and Markdown before a commit, and again
before a push, with cargo-deny when a manifest changed and actionlint and
zizmor when a workflow changed; none of them compiles, so they take seconds.
Before asking for review, run `cargo xtask check`: the code rules clippy
cannot express (ast-grep, from [`sgconfig.yml`](sgconfig.yml)), unused
dependencies, clippy and the doctests, and the tests of the packages the
change touches and of the packages that depend on them. Off Windows it also
lints the workspace for Windows through
[cargo-xwin](https://github.com/rust-cross/cargo-xwin). Most of the loader runs
only on Windows: off Windows its portable modules are tested on the host, and
`cargo xtask check --windows` runs the rest of its tests under Wine on an
amd64 Linux host that has it. There, use the Wine of
[GE-Proton](https://github.com/GloriousEggroll/proton-ge-custom): unpack a
release and put its `files/bin` first on `PATH`. Elsewhere those tests run on
Windows. CI runs them on a Windows runner for every pull request that touches
them. `cargo xtask check --all` runs the whole suite, the Wine tests included
where Wine runs. The
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
  with the change; `cargo xtask check` checks its structure, and its Rust
  examples run as doctests.

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

## Releases

Until the first public release, 1.0.0, this repository has no regular
releases. The platform's crates carry prerelease versions, `1.0.0-alpha.N`,
and a mod depends on `enshwick = "=1.0.0-alpha.N"`.

The
[dev-build workflow](.github/workflows/dev-build.yml)
runs on every push to `main` that changes the crates or the tools, and by
hand. It builds the commit with `cargo xtask dist` and replaces the files
of the one prerelease tagged `dev`, whose tag moves to that commit, so
`dev` always carries `main`. `enshwick.zip` there holds the stub and the
loader, and no mod: each mod ships in its own archive from its own
repository. `cargo xtask dist` writes the same archive into `target/dist` on
any machine with `cargo-xwin`.

Versions come from [release-plz](https://release-plz.dev), set up in
[release-plz.toml](release-plz.toml) and run by hand through the
[release-plz workflow](.github/workflows/release-plz.yml):

1. `release-pr` reads the conventional commits since each crate's last
   release, checks the public API with cargo-semver-checks, and opens or
   updates one release pull request with the new versions and each crate's
   `CHANGELOG.md`.
2. Once that pull request merges, `release` tags each released crate
   `<package>-v<version>`, such as `enshwick-v1.0.0-alpha.3`, makes its
   GitHub release, and publishes the public crates to crates.io. The loader
   and the stub are tagged only; they never go to crates.io.
