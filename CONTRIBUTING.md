# Contributing

The [rules for every enshwick repository](https://github.com/enshwick/.github/blob/main/CONTRIBUTING.md)
hold here too: branches, pull request titles, signed commits and what a pull
request carries. This guide adds how to build and test Enshwick.

Enshwick is a Rust workspace. You need Rust (`rust-toolchain.toml` installs the
pinned version through rustup), [lefthook](https://lefthook.dev),
[rumdl](https://github.com/rvben/rumdl),
[typos](https://github.com/crate-ci/typos), cargo-deny,
[actionlint](https://github.com/rhysd/actionlint),
[zizmor](https://docs.zizmor.sh),
[ShellCheck](https://www.shellcheck.net) and jq for the git hooks,
[ast-grep](https://ast-grep.github.io) and
[cargo-machete](https://github.com/bnjbvr/cargo-machete) for
`cargo xtask check`, and on Windows the MSVC build tools, whose Windows SDK
and CRT libraries the linker needs.

```bash
lefthook install                            # the git hooks
rumdl check .                               # every document, as the hooks check it
cargo install --locked cargo-nextest@0.9.152
cargo nextest run --workspace
```

The hooks check formatting, spelling and Markdown before a commit, and again
before a push, with cargo-deny when a manifest changed and actionlint, zizmor,
ShellCheck and a dry run of the Jira script when a workflow changed; none of
them compiles, so they take seconds.
Before asking for review, run `cargo xtask check`: the crate rules (a crate
whose package has no unsafe code declares `#![forbid(unsafe_code)]`, only a
dev-dependency turns on a crate's `test` feature, a package that builds a DLL
names its version), the code rules clippy
cannot express (ast-grep, from [`sgconfig.yml`](sgconfig.yml)), unused
dependencies, clippy and the doctests, clippy of each package it checks with
none of its features and with each one alone, and the tests of the packages
the change touches and of the packages that depend on them. Off Windows it also
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
  without them. `cargo xtask bench <name>` runs one benchmark: on a clean
  checkout of a `main` commit it keeps the bench executable under the
  commit's id; elsewhere it runs the newest kept `main` executable before
  the branch and then the branch's, back to back under the same load, so
  `main` needs no second build. The executables live in the clone's
  `target/kept-benches`, which its worktrees share, or in the folder
  `ENSH_KEPT_BENCHES` names;
- branch from `main` as `<type>/<slug>`, `<type>` a conventional commit
  type such as `feat`, `fix` or `chore`, and open a pull request into
  `main`; its title is a conventional commit, `<type>(<scope>): <subject>`,
  and becomes the squashed commit; the merge queue merges it once CI passes
  and `main`'s last run passed; while `main` is red only a `fix/` or revert
  branch merges, so the fix goes first;
- code follows the style of the code around it, unsafe code included;
- a pull request adds its changelog entries as a file of its own,
  `changes/<branch>.md` beside the `CHANGELOG.md` of each crate it changes,
  named after its branch, so parallel pull requests never edit one file. The file holds the Keep a Changelog headings it needs
  (`### Added`, `### Changed`, `### Deprecated`, `### Removed`, `### Fixed`,
  `### Security`), each once, with a list of entries under each;
  `cargo xtask check` refuses a file that does not read so, an edit to a
  change file already on `main`, and a change written under a changelog's
  Unreleased section by hand:

  ```markdown
  ### Added

  - `world`: `World::revive` stands a dead character up (#1036).
  ```

- a change to the API's public paths, an item added, renamed, moved or
  gated, rewrites `tools/public-paths.txt` with `cargo xtask public-paths`
  in the same pull request: the list is written from rustdoc's view of the
  crate, the
  [API coverage page](https://enshwick.com/dev/api-coverage/) checks the
  items it names against it, and `cargo xtask check` refuses a stale copy;
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

Never add game code or data: no decompiled code, no dumps, no binaries, no
reflection catalogues and no build tables. Engine facts go in documents as
addresses and names, with the game build they were found on. Keep the
catalogues and tables in your game data folder, which `ENSH_GAMEDATA` names,
outside the repository. The README lists
[what the repository holds of the game](README.md#what-the-repository-holds-of-the-game).

Security issues go through [SECURITY.md](SECURITY.md), not public issues.

## Releases

Until the first public release, 1.0.0, the loader ships as alphas. The
platform's crates carry prerelease versions, `1.0.0-alpha.N`, and a mod
depends on the alpha's tag from git (`tag = "v1.0.0-alpha.N"`). An alpha follows a merged change
that players, admins or mod authors get, at most one a day:

1. A pull request moves every crate to the next `1.0.0-alpha.N`, with the
   exact `=` pins between the published crates, and closes each
   `CHANGELOG.md` with `cargo xtask close --date <YYYY-MM-DD>`: each
   crate's change files join its changelog under
   `## [1.0.0-alpha.N] - <date>`, each heading's entries in the files' name
   order, and the files go; a crate without one gets the heading alone. A
   change file that merges after it joins the same section when `close`
   runs again.
   `cargo xtask tag-lines` moves the install and dependency lines of the
   Markdown (`--tag v…`, `tag = "v…"`) to the new tag; `cargo xtask check`
   fails a line that names another one.
2. Once it merges, the [alpha workflow](.github/workflows/alpha.yml), run by
   hand on `main`, makes the GitHub prerelease `v1.0.0-alpha.N`:
   `enshwick.zip` with the PDB and CycloneDX SBOM of each DLL and their build
   provenance. `cargo xtask notes` writes its notes: a warning that anything
   may change before 1.0, the game version
   the alpha was checked against, each crate's changelog section and the known
   issues the run is given. A version on GitHub or crates.io already is
   refused, and so is one whose changelogs lack its section or whose change
   files are not joined yet. The workflow scans the archive on VirusTotal.

`enshwick.zip` holds the stub and the loader, and no mod: each mod ships in
its own archive from its own repository. `cargo xtask dist` writes the same
archive into `target/dist` on any machine with `cargo-xwin`; the
[dev-build workflow](.github/workflows/dev-build.yml) builds it on every
push to `main` that changes the crates or the tools and keeps it as the
run's artefact.

Alphas go to GitHub only, never to crates.io. Real releases come from
[release-plz](https://release-plz.dev), set up in
[release-plz.toml](release-plz.toml) and run through the
[release-plz workflow](.github/workflows/release-plz.yml). It runs by hand
until the first real release; from then on, the repository variable
`RELEASE_ON_MERGE` set to `true` runs it on every merge to `main` that
changes the crates, `release` first and then `release-pr`. The variable
belongs to the repository, not to the `release` environment, whose
variables the switch does not see. `release` acts only on the merge commit
of a release pull request, so a failed release is redone by re-running that
run, not by a new run from a later `main`:

1. `release-pr` reads the conventional commits since each crate's last
   release, checks the public API with cargo-semver-checks, and opens or
   updates one release pull request with the new versions. The change files
   are the changelog, never the commits: release-plz writes only each
   released crate's `## [<version>] - <date>` heading, and the next step of
   the workflow runs `cargo xtask close --date <today> --package <crate>`
   for each crate the pull request releases, so their change files join
   that section at the crate's own version and go, and pushes the result to
   the release pull request.
2. Once that pull request merges, `release` tags each released crate
   `<package>-v<version>`, such as `enshwick-v1.0.0-alpha.3`, makes its
   GitHub release with the crate's joined changelog section as its notes,
   and publishes the public crates to crates.io. The loader
   and the stub are tagged only; they never go to crates.io.

Every crate release-plz releases keeps a `CHANGELOG.md`, the loader and the
stub included, since a crate without one gets a GitHub release with no
notes; `cargo xtask check` refuses a released crate without one, or with
its changelog turned off in [release-plz.toml](release-plz.toml). A version
whose section is empty, since none of its commits added a change file for the
crate, gets a release that says it lists no changes of its own. A change
to the loader or the stub that players and admins see takes a change file
beside the loader's or the stub's changelog, and what mod authors see of
it, one beside the changelog of `enshwick`.
