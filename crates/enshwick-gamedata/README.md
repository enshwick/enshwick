# enshwick-gamedata

Internal, not for mods: a part of [Enshwick](https://github.com/enshwick/enshwick). Mods
depend on [`enshwick`](https://crates.io/crates/enshwick) only.

The loader's build tables: per game build, the engine functions and constants
reflection does not describe, by symbolic id, as SKSE's Address Library does.
Each table is a TOML file in [`tables/`](https://github.com/enshwick/enshwick/tree/main/crates/enshwick-gamedata/tables), named
`<side>-<guid>-<age>.toml` after the build's fingerprint. A table also
records the build's Steam build id (`steam_build`) and the game's own
version number (`game_version`), each where it is known; several Steam
builds can share one game version.

The catalogue (`catalogue.rs`) maps the ids the API asks engine facts by,
such as `item_info.name`, to the game's reflected types and fields, so a
game update that moves a field changes this crate, not the mods.

- The loader embeds every table of this folder, and reads no other: a table
  for a newer game build ships with the next loader build.
- On a build without a table, the loader takes the newest table of the same
  side, by Steam build id, and finds each entry by its `pattern`: a symbol at or through the
  code it matches, a constant read from the operand of the instruction that
  uses it, or in the code of a symbol it names (`via`), at its start or
  once `within` a number of bytes from it. The slots of
  attributes name their `attribute` and come from the game's archive, which
  the loader reads on every build. A value whose pattern fixes the bytes
  that spell it says `fixed = true`, and a pattern with neither an operand
  nor `fixed` is refused. A value only its own build holds says
  `carried = false` and is left out.
  What neither finds stays missing, and only the features and mods that
  need it turn off.
- [`cargo enshwick pin`](https://github.com/enshwick/enshwick/blob/main/crates/cargo-enshwick/README.md#porting-build-rows-pin)
  writes the table for a new build, patterns included.

## Patterns

Every entry of the loader's tables carries a pattern that finds it on a
build without a table: a run of the game's code, unique in its code
section, with the bytes that move between builds left open (`??`): call and
jump targets, RIP-relative addresses, and field and stack offsets wherever
the run stays unique without them.

- A **symbol's** pattern is its own code, `at` bytes into the match. A
  function the game holds in several identical copies, or a global, is
  found through code that reaches it: `rel` names the offset of the call's,
  jump's or RIP-relative operand's rel32 in the match, and the symbol is
  where it points.
- A function whose code the loader replaces or relies on as a whole also
  carries a `body`: the function's length in bytes (`len`), the offsets of
  the 4-byte fields that move between builds (`open`: call and jump
  targets, RIP-relative addresses), and a hash of every other byte
  (`hash`, XXH3-128 in hex). Of the function's code the table holds only
  the short window that finds it: a window from its start over at most
  half of it, or a call to it (`rel`) for a short function or one whose
  start has a twin. The loader hashes the live code before it hooks or
  calls it. When Keen changes that function's logic, the hash differs,
  and the feature that relies on it turns off on
  that build instead of running over code it does not know.
- No pattern spans more than 192 bytes; a table with a longer one does not
  read.
- A **constant's** pattern is the instruction that uses the value, picked
  by hand as the use the loader relies on: a read of the field, the
  multiplier of a stride, the id passed to a lookup. The operand that
  carries the value is left open and read on the other build (`at` and
  `size`), so a field Keen moves comes back with its new offset. `neg` reads
  a frame slot below `rbp`, `shl` a size the code only shifts by, and `plus`
  a value next to the operand, as a size from the offset of the last field.
  A value no operand holds, such as an offset of 0 or a stride spelled by
  the instructions' scale, says `fixed = true`: its pattern fixes the
  instruction and confirms the value while that code stands. A pattern with
  neither an operand nor `fixed = true` does not read, so no carried value
  stays without a check.
- A symbol's expected code bytes (`<id>.bytes`) count where its pattern
  finds code that still starts with them.
- A value only a function too short to find on its own holds is read `via`
  that function: the function is a symbol found through a call to it, its
  code must match the constant's pattern where it starts, and the operand
  is read there. `knowledge.journal_blob` is read so, in the journal
  registry's two-instruction getter of its blob.
- A value in the body of a function the game holds in identical copies is
  read `via` that function `within` a number of bytes from its start: the
  pattern must match exactly once there, so a pattern no longer unique in
  the whole code still finds the instruction in its own copy. The
  interface's toggle config (`ui.toggle_*`), read only by the 45 copies of
  the toggle widget, is read so.
- The slot of an attribute in its set comes from the game's data, not from
  code: the game looks slots up at run time in its attribute container,
  which lists each set's attributes in slot order with their names. Such an
  entry names its `attribute` (`Health_Max`, `Fuel_Max`), and the loader
  reads the container from the game's archive at start, on every build; the
  value a table pins is a cross-check, and a slot that differs is logged and
  the game's data holds.

On build 1269579 the patterns find every symbol of both tables, 271 of the
server's 280 constants and 427 of the client's 436, and the game's data gives
the other nine, the slots of attributes: on a build without a table every
entry is found, and every loader feature and mod of this repository runs.
`cargo xtask compat --patterns <exe>` shows what the patterns alone find
on any executable, and a test of this crate checks, when
`ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name the game's executables, that
every pattern matches exactly once and yields its table's value.

### Adding a pattern

When an entry joins a table, pick the instruction that uses it, in the
build the table is for, and let the tool write the pattern:

```bash
cargo enshwick pin --patterns server/enshrouded_server.exe --write \
  --from "building.zone_entry_ignored=0x93e247" \
  --from "npc.lookup=ref:0x1f29fa"
```

`--from <id>=<pick>` (or `--from-file` with one pick per line) takes the RVA
of an instruction whose displacement or immediate carries the constant
(`disp:`, `neg:`, `imm:` or `shl:` before it to choose, `+<n>` after it for a
value next to it), `confirm:<rva>` for an instruction that only confirms it,
or, for a symbol, `ref:<rva>` of code that calls or addresses it, or `body`
to check its whole function by a hash (`body:<rva>` finds it through the
call at `rva`, for a function whose start has a twin). The tool
checks that the operand holds the table's value, grows the smallest unique
window around the instruction and writes it. Once written, the entry keeps
its instruction: later runs and
[`pin`](https://github.com/enshwick/enshwick/blob/main/crates/cargo-enshwick/README.md#porting-build-rows-pin)
for a new build find it by the pattern, or port it, and read the value
there. Constants without a pick get no pattern: a value seen in some code
says nothing about what that code means.
