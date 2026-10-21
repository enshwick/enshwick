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
- `manifest <dll>` prints what a built mod says about itself: id, version,
  side, join check and dependencies. With `--expect-version <version>` it fails
  when the DLL reports another version, which release pipelines can check.
- `install` builds the mod in release and copies its DLL to
  `<game>/mods/<package>.dll`. The game folder is `--game` or `ENSHROUDED_DIR`;
  in a workspace with several mods, `--package` names one.
- `id <mod:name>...` prints the GUID and the game id of each
  [content id](../../book/src/content.md#content-ids), as `Guid::named`
  derives them: `cargo enshwick id safes:safe`.
- `pin --old <exe> --new <exe>` ports the build rows of the loader and the
  mods in an Enshwick checkout to a new game build, for maintainers. See
  [below](#porting-build-rows-pin).

## Porting build rows (`pin`)

`cargo enshwick pin` ports the build rows to a new game build. Every game
update changes the build fingerprint and moves functions, and until rows for
the new build exist the loader leaves the game vanilla. This tool writes those rows where it
can prove them and lists the rest for a human. It never guesses.

It reads the rows from the sources: the loader's table in
[`crates/internal/loader/src/builds.rs`](../internal/loader/src/builds.rs) and
the `builds:` rows of every `manifest!` under `mods/`. It ports each row whose
fingerprint is the old executable's.

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

   `--write` puts each new row right after its old one. Without it the report
   carries the new rows instead. The exit code is 0 when nothing needs a
   human, 2 when something does, and 1 on an error.
4. **Review.** The report lists every entry with its verdict. Settle each item
   under "For a human": a dropped symbol has a `// pin:` note where it was, and
   a flagged constant has one above it. Read `git diff`, build, and try the
   new build in game.
5. **Open a pull request** with the new rows and the report's summary. Never
   commit an executable or anything else from the game.

### How it decides

- **Checks** are systems the loader compares with reflection: their new value
  is the reflected entry in the new executable.
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
- **Constants** keep their value. A constant at least `0x10` is `kept` when the
  row's matched code still uses it as a displacement or immediate. It is
  `flagged` when all its uses lie in code that did not match, and
  `unverified` when the row's code does not use it (or the value is too small
  to sample). A 64-bit constant that holds a symbol's first code bytes is
  compared with the bytes at the symbol's new address. Only flagged constants
  need a human. Unverified ones are worth a look when the report shows that
  the code around them moved a lot.

### Tests

`cargo nextest run -p cargo-enshwick` covers the signatures, the search and the
row parser. Two more tests need a game executable, which is never in the
repository:

```bash
ENSHWICK_PIN_EXE=path/to/enshrouded_server.exe cargo test --release -p cargo-enshwick -- --ignored
```

`a_build_ports_to_itself` ports every row of the executable's build to the
same executable, and every symbol and check must map to itself.
`moved_code_is_followed` moves all the code 16 bytes on, function table
included, and rewrites one function. Every other symbol in code must follow
the move, and the rewritten one must come out `missing`. The same command
works with `enshrouded.exe`.

### Game build watch

[`game-build-watch.yml`](../../.github/workflows/game-build-watch.yml) runs
weekly. It asks steamcmd for the public build id of the dedicated server and
opens an issue when the loader's table has no server row named after it. It
downloads no game files. It stays off until the repository variable
`GAME_BUILD_WATCH` is `true`.

## More

The [modding book](https://github.com/themaiby/enshwick/blob/main/book/src/SUMMARY.md) covers what goes into the mod.

Licensed under [MIT](../../LICENSE).
