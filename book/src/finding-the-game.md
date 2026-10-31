# Finding the game

Addresses in the game change with every update, so a mod never hard-codes one.
It asks the loader during `init`, by name, and the loader answers for the build
that runs. Every lookup returns `Result<_, Off>`: a mod that cannot find what
it needs returns the `Off` with `?` and stays off, and the game runs vanilla
where it would have acted.

| Call | Finds | Survives updates |
| --- | --- | --- |
| `load.system(name)` | an ECS system's update function, by its reflected name | yes |
| `load.field_offset(type, field)` | a field's offset in a reflected type | yes |
| `load.type_size(type)` | a reflected type's size | yes |
| `load.find_code(pattern)` | the one place in the game's code that matches a byte pattern | while those bytes stay |
| `load.symbol(id)` | an engine function or address pinned per build | the loader's ids by their patterns; a mod's own rows need a new row |
| `load.constant(id)` | an engine offset, size or value pinned per build | the loader's ids by their patterns; a mod's own rows need a new row |

## Reflection: systems, fields and sizes

The game describes its ECS systems and many types about itself. The loader
reads that reflection at start, so these lookups keep working after an update
without anyone touching the mod:

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let system = load.system("upgrade_building_range")?;
    let range_x = load.field_offset("keen::ecs::TriggerShape", "rangeX")?;
    let size = load.type_size("keen::ecs::TriggerShape")?;
    enshwick::info!("system at {system:#x}, rangeX at +{range_x}, {size} bytes");
    Ok(())
}
```

`system` returns the absolute address of the update function, ready for
[`load.hook`](hooks.md). A name that is missing or matches more than one
system or type gives an `Off` that says so.

## Pinned symbols and constants

Engine helpers that reflection does not describe are pinned per build under a
symbolic id, such as `npc.lookup` or `ecs.context_row`:

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let lookup = load.symbol("npc.lookup")?;
    let row = load.constant("ecs.context_row")?;
    enshwick::info!("lookup at {lookup:#x}, row word at +{row}");
    Ok(())
}
```

`symbol` looks in the mod's own build rows first, then in the loader's table,
and returns an absolute address: the row holds an offset from the game's image
base, and the loader adds the base. `constant` looks in the same two places and
returns the value as it is.

The loader's build tables hold what several mods share: the ECS iterator,
component reads, chat, labels, inventories and so on. They are data, one TOML
file per game build in
[`crates/enshwick-gamedata/tables`](https://github.com/enshwick/enshwick/tree/main/crates/enshwick-gamedata/tables),
as SKSE's Address Library is, embedded in the loader. A game update needs a
loader build with the new build's table; until then the loader finds each
entry of the newest table by its code pattern, and what no pattern finds stays
off. Ids a single mod uses go in that mod's rows.

## Build rows in the manifest

A mod pins its own ids with build rows in its
[manifest](concepts/loader-and-abi.md#the-manifest), the `Cargo.toml`:

```toml
[[package.metadata.enshwick.builds]]
side = "server"
guid = "1c129660cd6efc4f9f0b21652d5ff2bf"
age = 387

[package.metadata.enshwick.builds.symbols]
"my-mod.spawn_return" = 0x4_05cb

[package.metadata.enshwick.builds.constants]
"my-mod.bed_offset" = 0x48

[[package.metadata.enshwick.builds]]
side = "client"
guid = "69f0e9573b32c24a8838d3fff550824c"
age = 410

[package.metadata.enshwick.builds.symbols]
"my-mod.spawn_return" = 0x22_368b
```

- `side` is the binary the row is for: `"server"` for the dedicated server,
  `"client"` for the game. The game's binary holds twins of the server's
  functions, which run while it hosts a game, so a mod with server hooks pins
  them in a `"client"` row as well (see
  [Sides and hosting](concepts/sides-and-hosting.md)).
- `guid` and `age` identify the build: the GUID, as 32 lowercase hex digits,
  and the age of the CodeView record in the executable. The game's version
  string repeats across builds; this pair does not. The loader logs both when
  it meets a build it does not know.
- `symbols` map an id to an offset from the image base; `constants` map an id
  to a value. Either table may be left out.

A row applies only when its side and both parts of its fingerprint match the
running binary. Rows for other builds are ignored, so a mod can carry rows for
several builds at once.

[`enshwick-research`](https://github.com/enshwick/devtools-mods/blob/main/enshwick-research/src/lib.rs)
pins the client functions it studies in a `"client"` row and two server
functions in a `"server"` row.

When the game updates, the new build's table and rows come from
[`cargo enshwick pin`](https://github.com/enshwick/enshwick/blob/main/crates/cargo-enshwick/README.md#porting-build-rows-pin):
given the old and the new executable, it finds every pinned symbol of the
loader's table and of each mod's rows again, writes the new table file and
puts the new rows next to the old ones. What it cannot place for sure it
leaves out with a `pin:` note, for a human to settle. Rows that pin the start
of a function or a call site port best; an address in the middle of code that
changed does not.

## When the build is unknown or an id is missing

- **A build without a table.** The loader identifies the binary by its
  fingerprint and looks for the table made for it. When there is one, it
  checks a few systems of the table against reflection and loads nothing
  when they disagree. When there is none, as right after a game update, it
  takes the newest table of the same side and finds each entry by the code
  pattern the table keeps for it (see
  [Patterns of the loader's table](#patterns-of-the-loaders-table)). What no
  pattern finds is missing, so only the loader features and the mods that
  need it turn off, and the log names the build. With `build_patterns =
  false` in `mod-configs/enshwick.toml` such a build stays vanilla instead,
  and a mod that set a [hold](state.md) stops the game, so a world that needs
  the mod is not opened without it. A mod's own rows cannot make an unknown
  build known.
- **Missing id.** An id that neither the mod's rows nor the loader's table
  pins, or that no pattern found, gives `Off` with "missing on this game
  build". Only the mod that asked turns off; the others load.

Find every address before registering anything, as
[the loader's base limit](https://github.com/enshwick/enshwick/blob/main/crates/internal/loader/src/bases.rs)
does, so a miss leaves the game untouched rather than half changed.

## Patterns of the loader's table

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
on any executable, and a test of the builds crate checks, when
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

## Code patterns

`load.find_code(pattern)` searches the game's code section for a byte pattern
and returns the absolute address of the one place it matches. It survives an
update that leaves those bytes alone, without a new row.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let site = load.find_code("B9 00 02 00 00 66 3B C1 0F 84 ?? ?? ?? ??")?;
    enshwick::info!("check at {site:#x}");
    Ok(())
}
```

- A pattern is hex bytes separated by spaces, two digits each, with `??` (or
  `?`) for a byte that may be anything, such as a jump's displacement or a
  field offset that moves between builds.
- It needs at least one fixed byte.
- It must match exactly once. No match and more than one match both give `Off`,
  so a pattern never lands on the wrong place. Make it long enough to be unique.

[`code::Pattern`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/code.rs)
is the same parser and search, for use on any slice: `parse`, `matches`,
`find_all` and `find_unique`. `load.code()` returns the game's code section as
a `&[u8]` that starts at its absolute address, for a mod that reads the bytes
around a match during `init`; it borrows the `Load`, since arming writes into
the code afterwards:

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let code = load.code()?;
    let pattern = enshwick::code::Pattern::parse("E8 ?? ?? ?? ?? 84 C0")?;
    let calls = pattern.find_all(code).count();
    enshwick::info!("{calls} calls followed by a test of al");
    Ok(())
}
```

Patterns find places to [patch](code-patches.md) and to
[hook mid-function](mid-function-hooks.md), as well as functions to
[hook](hooks.md). Never put game code itself in a mod's source beyond the short
patterns it searches for.
