# enshwick-compat

A maintainer's tool of this repository, not published: it links the loader.

`enshwick-compat` tells what works on a game build, from its
executable, before anyone starts the game:

```bash
cargo run --release -p enshwick-compat -- server/enshrouded_server.exe --mods server/mods
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
