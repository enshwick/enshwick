# cargo-enshwick

A Cargo subcommand for writing [Enshwick](https://github.com/enshwick/enshwick)
mods. It starts a Rust mod, a Luau mod or a data pack, and it installs a mod
into the game.

```sh
cargo install --git https://github.com/enshwick/enshwick cargo-enshwick
cargo enshwick new you.my-mod --side server
cd my-mod
cargo enshwick install --game "C:/Program Files (x86)/Steam/steamapps/common/Enshrouded"
```

- `new <id>` creates a mod crate for the id `<author>.<name>` in
  `<dir>/<name>` (`--dir`, default the current folder) for `--side server`,
  `client` or `both`. The crate has a toolchain file, build flags, lints,
  texts and tests. It builds and loads as it is: settings in
  `mod-configs/<id>.toml`, a line in `logs/enshwick/mods/<id>.log`. It pins
  the `enshwick` release the tool was built with; `--enshwick-rev <commit>`
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
- `luau [--out <file>]` writes the API's Luau definitions for the loader the
  tool was built with, to refresh a mod's `types/enshwick.d.luau`.
- `manifest <dll>` prints what a built mod says about itself: id, version,
  side, join check and dependencies. With `--expect-version <version>` it fails
  when the DLL reports another version, which release pipelines can check.
- `install` builds the mod in release as a Windows DLL, through
  [cargo-xwin](https://github.com/rust-cross/cargo-xwin) off Windows, and
  copies it to `<game>/mods/<package>.dll`. The game folder is `--game` or `ENSHROUDED_DIR`;
  in a workspace with several mods, `--package` names one.
- `id <mod:name>...` prints the GUID and the game id of each
  [content id](https://github.com/enshwick/enshwick/blob/main/book/src/content.md#content-ids), as `Guid::named`
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
- `pin --old <exe> --new <exe>` writes the loader's build table for a new game
  build and ports the mods' build rows, in an Enshwick checkout, for
  maintainers. See [below](#porting-build-rows-pin).

## Porting build rows (`pin`)

`cargo enshwick pin` writes the build table of a new game build. Every game
update changes the build fingerprint and moves functions. Until the new
build's table exists, the loader runs it on the code patterns of the newest
table, and whatever they miss stays off. This tool writes the table where it
can prove each entry and lists the rest for a human. It never guesses.

It reads the loader's tables, one TOML file per build in
[`crates/enshwick-gamedata/tables`](https://github.com/enshwick/enshwick/tree/main/crates/enshwick-gamedata/tables), and the
`[[package.metadata.enshwick.builds]]` rows of every mod's `Cargo.toml`
under `mods/`. A row gives its symbols and constants as
`[package.metadata.enshwick.builds.symbols]` and
`[package.metadata.enshwick.builds.constants]` tables of `"id" = value`
lines; `pin` refuses a row that gives them as an inline table or dotted keys.
It ports each table and row whose fingerprint is the old executable's. Each entry of the new table also
gets the code pattern that finds it on a later build without a table, taken
from the new executable: the smallest window of instructions that is unique
in the code, with moving parts masked, and field and frame offsets too where
the window stays unique without them. A symbol's window is its own code, or
the code that calls or addresses it; a constant's is the instruction picked
as its use, with the operand that carries it left open to read. A constant
read `via` a symbol is read again in that symbol's code, and a slot named by
its `attribute` is kept as it is (see
[Patterns](https://github.com/enshwick/enshwick/blob/main/crates/enshwick-gamedata/README.md#patterns)).

### Workflow

1. **Keep the old executables.** The tool needs the executable the current
   rows were made for. Copy `enshrouded_server.exe` and `enshrouded.exe`
   aside before you update.
2. **Download the new build** with
   [steamcmd](https://developer.valvesoftware.com/wiki/SteamCMD). Anyone can
   download the dedicated server (app 2278520); the client (app 1203620) needs
   an account that owns the game:

   ```bash
   steamcmd +force_install_dir ./server +login anonymous +app_update 2278520 validate +quit
   steamcmd +force_install_dir ./client +login <account> +app_update 1203620 validate +quit
   ```

   The Steam build id is in `steamapps/appmanifest_2278520.acf` (`buildid`),
   and in `appmanifest_1203620.acf` for the client.
3. **Run the tool** from the repository root, once per executable:

   ```bash
   cargo run --release -p cargo-enshwick -- enshwick pin \
     --old old/enshrouded_server.exe --new server/enshrouded_server.exe \
     --steam-build 26100000 --write --report pin-server.md
   cargo run --release -p cargo-enshwick -- enshwick pin \
     --old old/enshrouded.exe --new client/enshrouded.exe \
     --steam-build 26100001 --write --report pin-client.md
   ```

   `--write` writes the new table file beside the old one and puts each new
   mod row right after its old one; it needs `--steam-build`. The table
   records it as `steam_build`, and the game's own version number as
   `game_version` when `--game-version` gives it. Without `--write` the report
   carries the new table and rows instead. It writes every file or none:
   when one cannot be written, the others stay as they were. The exit code is 0 when nothing
   needs a human, 2 when something does, and 1 on an error.
4. **Review.** The report lists every entry with its verdict. Settle each item
   under "For a human": a dropped entry has a `pin:` note (above its table's
   header in the table file, where it was in a mod's row), and a flagged
   constant has one above it. Read `git diff`, build, and try the new build in
   game.
5. **Open a pull request** with the new table, the rows and the report's
   summary. Never commit an executable or anything else from the game. Once
   it is merged, the next loader build embeds the table.

`--patterns <exe>` rewrites the patterns of the table for that executable's
build from the executable itself. With `--write` it writes the table file;
without, it prints it. `--from <id>=<pick>`, or `--from-file` with one
`<id> <pick>` per line, builds an entry's pattern around an instruction:

| Pick | For | The pattern |
| --- | --- | --- |
| `<rva>` | a constant | reads the displacement or immediate of the instruction at `rva` that holds the value |
| `disp:`, `neg:`, `imm:`, `shl:<rva>` | a constant | reads that operand: the displacement, the displacement negated (`[rbp - value]`), the immediate, or `1 << immediate` |
| `<rva>+<n>`, `<rva>-<n>` | a constant | reads the operand and adds `n`, for a value next to it |
| `confirm:<rva>` | a constant | fixes the instruction, which confirms the value while it stands |
| `ref:<rva>` | a symbol | follows the rel32 of the call, jump or RIP-relative operand at `rva` |
| `direct` | a symbol | its own code |
| `body`, `body:<rva>` | a symbol | its own code, or the call at `rva`, plus a hash of its whole function (`body = { len, open, hash }`), so a change to its logic fails the check |

An entry keeps its instruction from then on: each run finds it again by the
entry's pattern. A constant without a pick gets no pattern, as a value found
in some code says nothing about what the code means. The exit code is 2 when
a pick gives no pattern, with the reason.

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

### Tests

`cargo nextest run -p cargo-enshwick` covers the signatures, the search and the
row parser. Three more tests need a game executable, which is never in the
repository:

```bash
ENSHWICK_PIN_EXE=path/to/enshrouded_server.exe cargo test --release -p cargo-enshwick -- --ignored
```

`a_build_ports_to_itself` ports every table and row of the executable's build
to the same executable, and every symbol and check must map to itself.
`moved_code_is_followed` moves all the code 16 bytes on, function table
included, and rewrites one function. Every other symbol in code must follow
the move, and the rewritten one must come out `missing`.
`a_moved_field_is_read_on_the_new_build` moves three fields the table reads
from code by 8 bytes: porting and the patterns alone must both give the new
offsets. The same command works with `enshrouded.exe`. With `ENSH_SERVER_EXE`
and `ENSH_CLIENT_EXE` set, `cargo nextest run -p enshwick-gamedata`
checks that every pattern of each table matches exactly once on its own
build and yields the table's value, and that the patterns cover at least as
many entries as they do today.

## More

The [modding book](https://github.com/enshwick/enshwick/blob/main/book/src/SUMMARY.md) covers what goes into the mod.

Licensed under [MIT](https://github.com/enshwick/enshwick/blob/main/LICENSE).
