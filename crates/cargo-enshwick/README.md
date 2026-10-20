# cargo-enshwick

A Cargo subcommand for writing [Enshwick](../../README.md) mods.

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

The [modding guide](../enshwick-api/GUIDE.md) covers what goes into the mod.

Licensed under [MIT](../../LICENSE).
