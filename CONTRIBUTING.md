# Contributing

The [rules for every enshwick repository](https://github.com/enshwick/.github/blob/main/CONTRIBUTING.md)
hold here too: branches, pull request titles, signed commits and what a pull
request carries. This guide adds how to build and test Enshwick.

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
cargo install --locked cargo-nextest@0.9.151
cargo nextest run --workspace
```

The hooks check formatting, spelling and Markdown before a commit, and again
before a push, with cargo-deny when a manifest changed and actionlint and
zizmor when a workflow changed; none of them compiles, so they take seconds.
Before asking for review, run `cargo xtask check`: the crate rules (a crate
whose package has no unsafe code declares `#![forbid(unsafe_code)]`, only a
dev-dependency turns on a crate's `test` feature, a package that builds a DLL
names its version), the code rules clippy
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
  examples run as doctests;
- a chapter opens with a comment that gives its page on
  [enshwick.com](https://enshwick.com/dev/) a type (`tutorial`, `how-to`,
  `reference` or `explanation`) and a description of 120 to 160 characters,
  and `cargo xtask check` refuses a chapter without them:

  ```markdown
  <!--
  type: how-to
  description: A hook puts a function of the mod in front of a game function, for what the API has no call for. It may change the arguments, call on, or change the result.
  -->
  ```

- the book's links to files and folders of this repository and of
  [enshwick/mods](https://github.com/enshwick/mods) name ones that exist:
  `cargo xtask check` looks the mods' up in the clone `ENSH_MODS_DIR` names,
  or a `mods` clone beside this one, and skips them with a note without one.

What a mod needs and another mod could use goes into `enshwick`, not into
the mod: events, world reads, content such as items and recipes, texts. A mod
keeps only what is its own. The API covers common needs at a high level and
always leaves the lower levels open (served resources, component reads,
detours on any function), so no mod is blocked by what the API lacks yet.
Resources several mods may change compose: their patches run in turn.

To ask for an API call, open an
[API request](https://github.com/enshwick/enshwick/issues/new?template=api-request.yml).
It asks for the game system, what the mod needs to do, its language and an
example. Before a large pull request, open one first, so nobody builds the
same thing twice.

Never add game code or data: no decompiled code, no dumps, no binaries. Engine
facts go in documents as addresses and names, with the game build they were
found on.

Security issues go through [SECURITY.md](SECURITY.md), not public issues.

## Releases

Until the first public release, 1.0.0, the loader ships as alphas. The
platform's crates carry prerelease versions, `1.0.0-alpha.N`, and a mod
depends on the alpha's tag from git (`tag = "v1.0.0-alpha.N"`). An alpha follows a merged change
that players, admins or mod authors get, at most one a day:

1. A pull request moves every crate to the next `1.0.0-alpha.N`, with the
   exact `=` pins between the published crates, and closes each
   `CHANGELOG.md`'s Unreleased section as `## [1.0.0-alpha.N] - <date>`.
2. Once it merges, the [alpha workflow](.github/workflows/alpha.yml), run by
   hand on `main`, makes the GitHub prerelease `v1.0.0-alpha.N`:
   `enshwick.zip` with the PDB and CycloneDX SBOM of each DLL and their build
   provenance. `cargo xtask notes` writes its notes: a warning in English and
   Ukrainian that anything may change before 1.0, the game builds the
   loader's tables support, each crate's changelog section and the known
   issues the run is given. A version on GitHub or crates.io already is
   refused, and so is one whose changelogs lack its section or still hold
   changes under Unreleased. The workflow scans the archive on VirusTotal.

`enshwick.zip` holds the stub and the loader, and no mod: each mod ships in
its own archive from its own repository. `cargo xtask dist` writes the same
archive into `target/dist` on any machine with `cargo-xwin`; the
[dev-build workflow](.github/workflows/dev-build.yml) builds it on every
push to `main` that changes the crates or the tools and keeps it as the
run's artefact.

Alphas go to GitHub only, never to crates.io. Real releases come from
[release-plz](https://release-plz.dev), set up in
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
