# cargo-enshwick

A Cargo subcommand for writing [Enshwick](https://github.com/enshwick/enshwick)
mods. It starts a Rust mod, a Luau mod or a data pack, and it installs a mod
into the game.

```sh
cargo binstall --git https://github.com/enshwick/enshwick cargo-enshwick@1.0.0-alpha.15
cargo enshwick new you.my-mod --side server
cd my-mod
cargo enshwick install --with-loader
```

[cargo-binstall](https://github.com/cargo-bins/cargo-binstall) downloads the
tool from the release: Windows and Linux binaries, with their SHA-256
checksums, SBOM and build provenance on the release. Without it,
`cargo install --git https://github.com/enshwick/enshwick --tag v1.0.0-alpha.15 cargo-enshwick`
builds the tool from source. The tool says when a newer release of itself
is out, at most once a day; without a network it says nothing, and `CI` or
`CARGO_ENSHWICK_NO_UPDATE_CHECK` set turns the check off.

- `new <id>` creates a mod crate for the id `<author>.<name>` in
  `<dir>/<name>` (`--dir`, default the current folder) for `--side server`,
  `client` or `both`. The crate has a toolchain file, build flags, lints,
  texts and tests. It builds and loads as it is: settings in each world's
  `<stem>-mod-settings/<id>/settings.toml`, a line in
  `logs/<id>_<date>_<time>.log`. It pins
  the `enshwick` release the tool was built with, an alpha's tag from git; `--enshwick-rev <commit>`
  takes a commit of this repository's `main` branch from git instead.
  Inside a Cargo workspace the crate joins it: its folder goes into the
  workspace's `members` unless a pattern there covers it, and the release
  profile, the toolchain and the build flags are left to the workspace root.
- `new --data <id>` creates a data pack folder `<dir>/<id>` instead: every
  pack file with its first change commented out, their JSON Schemas and a
  README. It checks clean as it is.
- `new --luau <id>` creates a [Luau mod](https://github.com/enshwick/enshwick/blob/main/book/src/luau.md)
  folder `<dir>/<id>`: `pack.toml`, a `main.luau`, the API's definitions in
  `types/enshwick.d.luau`, the luau-lsp settings that read them and a
  README. It runs on the server unless `--side` says otherwise;
  `--game <game>` adds the game's own types in `types/game.d.luau`.
- `check [dir]` checks a mod in any form, in the current folder unless
  `dir` names one. A data pack or Luau mod is read as the loader reads it,
  against the game's data with `--game` or `ENSHROUDED_DIR`, and every
  script is type-checked with luau-lsp against the definitions in `types/`.
  A Rust mod is linted with clippy for Windows, which also checks its
  `[package.metadata.enshwick]` table. Each problem is printed as
  `<file>:<line>:<column>: <severity>: <message>`, and an error exits with 1.
  Without luau-lsp on the path, `check` downloads the release it was built
  for once into the user's cache folder and checks its SHA-256; `--offline`
  skips the download.
- `luau [--out <file>]` writes the API's Luau definitions for the loader the
  tool was built with, to refresh a mod's `types/enshwick.d.luau`.
- `manifest <dll>` prints what a built mod says about itself: id, version,
  side, join check and dependencies. With `--expect-version <version>` it fails
  when the DLL reports another version, which release pipelines can check.
- `install` builds the mod in release as a Windows DLL, through
  [cargo-xwin](https://github.com/rust-cross/cargo-xwin) off Windows, and
  copies it to `<game>/mods/<id>/<package>.dll`. The game folder is `--game` or `ENSHROUDED_DIR`,
  else the one Steam installed: the game (app 1203620) when it is
  installed, else the dedicated server (app 2278520); the others are listed. In a
  workspace with several mods, `--package` names one. `--with-loader` then puts
  in the loader of the tool's release when the folder has none: the
  release's `enshwick.zip` from GitHub, checked against its `SHA256SUMS` once
  and kept in the tool's cache folder. A loader of another release is named
  and left as it is.
- `id <mod:name>...` prints the GUID and the game id of each
  [content id](https://github.com/enshwick/enshwick/blob/main/book/src/concepts/ids.md), as `Guid::named`
  derives them: `cargo enshwick id you.lanterns:lantern`.
- `deps [<alias>] --from <path> [--luau]` writes what each dependency
  offers, its
  [extension points](https://github.com/enshwick/enshwick/blob/main/book/src/extending.md#getting-a-points-definition),
  calls and mod events, as declarations in `src/deps/<alias>.rs`. It reads
  the schemas the owner's points, calls and events put into the owner's DLL,
  without running it. `--from` is the owner's DLL, a folder with it in it or
  in its `mods/` (such as the game's), or a schema file the owner published.
  With `--luau` it writes a Luau module per point, call and event for a Luau
  mod instead.
- `types [<name>...] --from <game> [--rust <file>] [--luau <file>]` writes
  the [game's own types](https://github.com/enshwick/enshwick/blob/main/book/src/game-types.md)
  as Rust and Luau from the reflection data of the game executable `--from`
  names or holds: the named types or namespaces (ending in `::`) and every
  type they use, or every runtime type when none is named.
- `pack check <dir>` checks a [data pack](https://github.com/enshwick/enshwick/blob/main/book/src/content/packs.md)
  folder as the loader reads it, every problem with its file and line;
  `pack schema [dir]` writes the JSON Schema of each pack file for editors;
  `pack zip <dir> [--out <file>]` checks the pack and zips it for a release,
  its files under `mods/<id>/`.
- `pin --old <exe> --new <exe>` ports a mod's own build rows to a new game
  build. See [below](#porting-build-rows-pin).

## Porting build rows (`pin`)

`cargo enshwick pin` ports a mod's own build rows to a new game build. Most
mods have none: the loader finds its entries on every build by its engine
routes and anchors. A mod that pins addresses or constants of its own does
so per build, in `[[package.metadata.enshwick.builds]]` rows of its
`Cargo.toml`, and every game update changes the build fingerprint and moves
functions. `pin` writes the new rows where it can prove each entry and lists
the rest for a human. It never guesses.

```toml
[[package.metadata.enshwick.builds]]
side = "server"
guid = "f1e760a53ff247409ab91462e88b8f49"
age = 224

[package.metadata.enshwick.builds.symbols]
"you.mod.spawn" = 0x1f29fa

[package.metadata.enshwick.builds.constants]
"you.mod.stack_size" = 0x48
```

A row gives its symbols and constants as
`[package.metadata.enshwick.builds.symbols]` and
`[package.metadata.enshwick.builds.constants]` tables of `"id" = value`
lines; `pin` refuses a row that gives them as an inline table or dotted keys.
It reads every `Cargo.toml` below `--dir` (the current folder by default),
outside `target/` and hidden folders, so it ports one mod or a whole
workspace, and ports each row whose fingerprint is the old executable's:

1. **Keep the old executable.** The tool needs the executable the current
   rows were made for. Copy `enshrouded_server.exe` or `enshrouded.exe` aside
   before you update the game.
2. **Run the tool** once per executable:

   ```bash
   cargo enshwick pin --old old/enshrouded_server.exe \
     --new server/enshrouded_server.exe --write --report pin-server.md
   ```

   `--write` puts each new row right after its old one; without it the
   report carries the new rows instead. It writes every file or none: when
   one cannot be written, the others stay as they were. The exit code is 0
   when nothing needs a human, 2 when something does, and 1 on an error.
3. **Review.** The report lists every entry with its verdict. Settle each
   item under "For a human": a dropped entry has a `pin:` note where it was
   in the row, and a flagged constant has one above it. Build the mod and
   try it on the new build.

The platform's maintainers port the loader's own entries with
`cargo xtask compat pin` in the Enshwick repository, by the same rules (see
[Porting to a new build](https://github.com/enshwick/enshwick/blob/main/crates/enshwick-gamedata/README.md#porting-to-a-new-build)).

### How it decides

- **Checks** are systems the loader compares with reflection: their new value
  is the reflected entry in the new executable.
- **Values** past `0x7fff_ffff_ffff_ffff` are written as hex strings, as TOML
  integers are signed.
- **Symbols.** The tool decodes the code around the old address with
  [iced-x86](https://github.com/icedland/iced) and masks what moves between
  builds: RIP-relative displacements, rel32 branch and call targets, and
  absolute pointers.
  1. The whole function (its `.pdata` entry, masked) must match exactly one
     function of the same length in the new build: `high` confidence. Copies
     that match equally are told apart by what they call and read.
  2. When the function changed, the smallest window of instructions around
     the address that is unique in the old build must occur exactly once in
     the new build. The match must lie inside a function, and at its start
     when the address is a function entry: `medium` confidence.
  3. When the code is ambiguous (the same function compiled many times) or
     the address is data, the tool follows the old build's references to it:
     each call site or RIP-relative use is ported in turn, and the new
     instructions must all point at one place. That place must be one of the
     matching copies: `medium` confidence.

  Anything else is `ambiguous` or `missing` and stays out of the new row. When
  a changed function's callers still agree on its new address, the note gives
  that address to check by hand.
- **Constants with a pattern that reads them** take the value their
  instruction holds in the new build: the old pattern finds the instruction
  while the code around it holds, else the instruction is ported like a
  symbol. A changed value is `read`; the same one is `kept`. A constant whose
  pattern only confirms it is `kept` while the pattern still matches once.
- **Other constants** keep their value. A constant at least `0x10` is `kept` when the
  row's matched code still uses it as a displacement or immediate. It is
  `flagged` when all its uses lie in code that did not match, and
  `unverified` when the row's code does not use it (or the value is too small
  to sample). A 64-bit constant that holds a symbol's first code bytes is
  compared with the bytes at the symbol's new address. Only flagged constants
  need a human. Unverified ones are worth a look when the report shows that
  the code around them moved a lot.

## More

The [modding book](https://github.com/enshwick/enshwick/blob/main/book/src/SUMMARY.md) covers what goes into the mod.

Licensed under [MIT](https://github.com/enshwick/enshwick/blob/main/LICENSE).
