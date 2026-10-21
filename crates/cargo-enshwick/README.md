# cargo-enshwick

A Cargo subcommand for writing [Enshwick Loader](../../README.md) mods, and
the one tool of the repository.

```sh
cargo install --git https://github.com/themaiby/enshwick cargo-enshwick
cargo enshwick new my-mod --side server
cd my-mod
cargo enshwick install --game "C:/Program Files (x86)/Steam/steamapps/common/Enshrouded"
```

- `new <id>` creates a mod crate in `<dir>/<id>` (`--dir`, default the current
  folder) for `--side server`, `client` or `both`. It builds and loads as it is:
  settings in `mod-configs/<id>.toml`, a line in `logs/enshwick/mods/<id>.log`.
  Inside a Cargo workspace the crate joins it: its folder goes into the
  workspace's `members` unless a pattern there covers it, and the release
  profile is left to the workspace root.
- `manifest <dll>` prints what a built mod says about itself: id, version,
  side, join check and dependencies. With `--expect-version <version>` it fails
  when the DLL reports another version, which release pipelines can check.
- `install` builds the mod in release and copies its DLL to
  `<game>/mods/<package>.dll`. The game folder is `--game` or `ENSHROUDED_DIR`;
  in a workspace with several mods, `--package` names one.
- `id <mod:name>...` prints the GUID and the game id of each
  [content id](../../book/src/content.md#content-ids), as `Guid::named`
  derives them: `cargo enshwick id safes:safe`.
- `pack check <dir>` checks a [content pack](../../book/src/content/packs.md)
  folder as the loader reads it, every problem with its file and line;
  `pack schema [dir]` writes the JSON Schema of each pack file for editors;
  `pack zip <dir> [--out <file>]` checks the pack and zips it for a release,
  its files under `mods/<id>/`.
- `pin --old <exe> --new <exe>` writes the loader's build table for a new game
  build and ports the mods' build rows, in an Enshwick checkout, for
  maintainers. See [below](#porting-build-rows-pin).
- `compat <exe> [--mods <dir>]` reports which loader features and mods work on
  a game build. See [below](#compatibility-report-compat).

## Compatibility report (`compat`)

`cargo enshwick compat` tells what works on a game build, from its
executable, before anyone starts the game:

```bash
cargo enshwick compat server/enshrouded_server.exe --mods server/mods
```

- **Build table.** It takes the table the loader would take: the one made
  for the build, checked against the executable (its systems against
  reflection, each pattern and the expected bytes where the table pins
  them), or the newest table of the same side carried over by patterns, with
  the entries they miss. The slots of attributes come from the game's
  archive beside the executable, as the loader reads them at start (a
  Windows build of the tool).
- **Loader features.** Each part of the loader with the table entries it
  names, and which the build lacks: works, partly or off. Every part declares
  its entries in a `TABLE_IDS` list beside its code, and the loader's tests
  hold each list to the code and the tables. Needs a Windows build of the
  tool.
- **Mods.** With `--mods` (DLLs or folders of them) it runs the loader itself
  on the executable, in a process of its own: the image mapped at its base,
  the mods resolved and initialised as at a start, nothing hooked and no
  mod's `ready` run. Each mod that turns off says why, as in its log.
  Starting mods needs Windows.

`--patterns` takes the table by patterns even when one is made for the build,
to see what the patterns alone carry. `--report <file>` writes the Markdown report there. The
exit code is 0 when everything works, 2 when something does not, and 1 on an
error.

## Porting build rows (`pin`)

`cargo enshwick pin` writes the build table of a new game build. Every game
update changes the build fingerprint and moves functions. Until the new
build's table exists, the loader runs it on the code patterns of the newest
table, and whatever they miss stays off. This tool writes the table where it
can prove each entry and lists the rest for a human. It never guesses.

It reads the loader's tables, one TOML file per build in
[`crates/internal/builds/tables`](../internal/builds/tables), and the
`builds:` rows of every `manifest!` under `mods/`. It ports each table and row
whose fingerprint is the old executable's. Each entry of the new table also
gets the code pattern that finds it on a later build without a table, taken
from the new executable: the smallest window of instructions that is unique
in the code, with moving parts masked, and field and frame offsets too where
the window stays unique without them. A symbol's window is its own code, or
the code that calls or addresses it; a constant's is the instruction picked
as its use, with the operand that carries it left open to read. A constant
read `via` a symbol is read again in that symbol's code, and a slot named by
its `attribute` is kept as it is (see
[Patterns of the loader's table](https://github.com/themaiby/enshwick/blob/main/book/src/finding-the-game.md#patterns-of-the-loaders-table)).

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

   The Steam build id is in `steamapps/appmanifest_2278520.acf` (`buildid`).
3. **Run the tool** from the repository root, once per executable:

   ```bash
   cargo run --release -p cargo-enshwick -- enshwick pin \
     --old old/enshrouded_server.exe --new server/enshrouded_server.exe \
     --build 1300000 --write --report pin-server.md
   cargo run --release -p cargo-enshwick -- enshwick pin \
     --old old/enshrouded.exe --new client/enshrouded.exe \
     --build 1300000 --write --report pin-client.md
   ```

   `--write` writes the new table file beside the old one and puts each new
   mod row right after its old one; it needs `--build`. Without it the report
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
and `ENSH_CLIENT_EXE` set, `cargo nextest run -p enshwick-loader-builds`
checks that every pattern of each table matches exactly once on its own
build and yields the table's value, and that the patterns cover at least as
many entries as they do today.

### Game build watch

[`game-build-watch.yml`](../../.github/workflows/game-build-watch.yml) runs
weekly, never on pull requests. It asks steamcmd for the public build id of
the dedicated server. When no server table names it in `build`, a Windows job
downloads that server, runs `compat` on it with the mods of the latest
platform release and opens an issue with the report. Game files stay on the
runner. It stays off until the repository variable `GAME_BUILD_WATCH` is
`true`.

## More

The [modding book](https://github.com/themaiby/enshwick/blob/main/book/src/SUMMARY.md) covers what goes into the mod.

Licensed under [MIT](../../LICENSE).
