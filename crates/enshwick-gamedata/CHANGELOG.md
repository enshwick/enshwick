# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Added

- `Routes`, `Routed`, `Routing`, `Resolved::route`, `Resolved::routed` and
  `Reflection::systems`: engine routes find table entries in the game's own
  code through its reflected ECS systems, its debug strings and decoded
  instruction shapes, the same way on every build; `anchors/classes.toml`
  classes every table entry by how it anchors to the game (#918).
- Engine routes find a function through `calls`, an ECS system record by
  `system` alone, a shape in any routed function through `in`, and
  `called_by` over route ids; shapes match index registers, exact
  displacements and exact immediates; a taken constant takes `shift` and
  `plus`. A route's `check` is a shape the code at its symbol must start
  with, so the route stands for the symbol's table checks, and
  `Routes::checks` tells whether a route checks its symbol's code;
  `make_check`, `check_holds` and `shape_checks` make and test checks with
  one `ShapeNames`; `?r`, `?i`, `?b` and `?m` match any register,
  immediate, branch target or memory operand only.
  The route finder indexes code outside `.pdata`, and the routes find 720
  client and 499 server entries without a table row (#920).
- `Body`, `Resolved::body` and `MAX_PATTERN`: a symbol whose whole function
  the loader relies on is checked by the function's length, its moving
  4-byte fields and a hash of the rest (#710).
- `reports/` holds, for each build table, what the loader finds on its
  build: Steam build, game version, entry and route counts, the entries it
  lacks and the loader features they turn off (#922).

### Changed

- Breaking: a shape that matches twice finds nothing, and `Routes::ids`
  lists each entry once, sorted by id, without waypoints (#920).
- Breaking: `Routed::Reached` is a symbol a route reached without matching
  its code; it must start a function when it lies in the code, and
  `Resolved::route` supplies it without counting it routed, so its table's
  checks stay. A taken constant needs a two-step shape, an exact operand or
  an `{entry}`. A route that disagrees with the table stands for nothing to
  the routes that name it, and an ECS record name with two entries finds
  nothing. A start shape checks its symbol's code only over two steps and
  eight bytes; a `check` follows a `jmp` to its target and must cover eight
  bytes past it or end the function; a memory operand may name its size
  (`dword [rcx+0xb8]`). A symbol a route supplies on a carried table drops
  the table's head pattern, body and `.bytes` (#920).
- Breaking: a body row reads `body = { len, open, hash }` instead of
  `body = true` over a pattern of the whole function, and `Symbol::body`
  returns `Option<&Body>`. A body row needs a pattern that finds it: its own
  window over at most half of it, or a call to it (`rel`) (#710).
- Breaking: a table pattern spans at most 192 bytes; a longer one does not
  read (#710).
- `Table::fall_back` carries a body symbol only where its function still
  holds its body (#710).
- Breaking: `Table::restore` takes the code, its RVA and the cell reader
  `Table::fall_back` takes, and checks every entry against the code as the
  search does: the save keeps where each pattern matched, not the values,
  and a pattern that does not match there turns the save away. A save of
  the earlier format is not read and is searched again (#882).
- Breaking: `Table::carry_key` takes a range of the code it leaves out of
  the key, for bytes that change from start to start (#864).

### Removed

- Breaking: `Table::carry_key`, `CarryKey`, `Table::restore` and
  `Resolved::to_saved` are gone with the loader's saved carried table;
  `Table::fall_back` searches every time (#912).

## [1.0.0-alpha.11] - 2026-10-28

### Added

- `COMFORT_FURNITURE_ITEM_TAG` and its row in `RECIPE_KIND_TAGS`; an earlier
  row wins for an item with two kind tags (#628).

### Changed

- Breaking: the catalogue's id constants (`ITEM_ID`, `SLOT_TYPE`,
  `USED_ITEM` and the rest) are gone; `catalogue::ids` names every id once,
  as the API's engine ids do (`ids::ITEM_INFO_ID`, `ids::ITEM_SLOT_TYPE`,
  `ids::USED_ITEM`) (#636).

## [1.0.0-alpha.10] - 2026-10-28

### Added

- Catalogue facts for an item's tags (`item_info.tags`, `item_tag_weight`,
  `item_tag_weight.tag`), the Decorative item tag and
  `RECIPE_KIND_TAGS` with `recipe_kind_tags`, the kinds an item takes from
  its tags (#616).

### Changed

- `server.permission_entry_size` masks the permission table's entry count
  in its pattern, so the row carries over to a build whose table holds more
  or fewer entries (#617).

## [1.0.0-alpha.9] - 2026-10-27

### Added

- `Table::carry_key`, `Resolved::to_saved` and `Table::restore` save a
  carried table and read it back while the code and the table stay the
  same (#598).
- A constant row may name a `cell`, the symbol of the data the game keeps
  the value in; `Constant::cell` returns it (#603).
- Every table reads `skill.airborne_architect` from the cell the fly mode
  gate loads, and the client tables carry `user.interaction_kind` and
  `user.join_failed_kind` by pattern (#603).
- The catalogue names the map's marker types: `MapMarkerType`'s size,
  id and flags, the registry's type array and the `Visible` visibility
  (#607).
- The tables pin, by pattern, the map marker list and its entries in the
  client tables, the knowledge log entry's id and value, the placement
  cursor's altar flag and bit, the permission table's size, and the
  newest tables the voxel material numbering (#607).

### Changed

- `Table::fall_back` searches every pattern in one pass over the code:
  about 5 ms instead of 38 ms on a dedicated server, 7 ms instead of 84 ms
  on a client (#598).
- Breaking: `Table::fall_back` takes a reader of the `u32` the image holds
  at an RVA, for constants in cells (#603).

### Removed

- Breaking: the client tables no longer pin `net.provider_join_slot` (#603).

### Fixed

- The client 426 table pins `map.fog_destroy.bytes` as a constant, so the
  fog hook checks its bytes; `Table::parse` refuses pinned bytes among the
  symbols (#600).

## [1.0.0-alpha.8] - 2026-10-27

### Added

- `Image::relocations` lists the 64-bit addresses Windows moves when it
  maps an image away from its preferred base (#555).

## [1.0.0-alpha.7] - 2026-10-26

### Added

- The catalogue names an item's flags and `ItemCategory`, which selections
  of items read, and every member of `keen::ItemCategory` (#540).
- The tables of server 399 and client 426 pin `voxel.read_box` and
  `voxel.store` (#541).

## [1.0.0-alpha.6] - 2026-10-26

### Added

- The catalogue names the loot table types and fields that data pack loot
  changes resolve (#523).

## [1.0.0-alpha.5] - 2026-10-26

### Added

- Reads the constants of reflected enums by name, and the base type of a
  reflected struct, so the loader resolves recipe kinds, event types, damage
  types and game settings values without numbers in code (#461, #469, #494).
- Catalogue facts for game events, recipe kinds, world, inventory, loot,
  stats and game settings (#461, #465, #469, #494).

## [1.0.0-alpha.4] - 2026-10-26

### Added

- An engine catalogue that the loader and `cargo enshwick` share, resolving
  engine types, sizes and field offsets by enshwick ids from the game's
  reflection (#460).

## [1.0.0-alpha.3] - 2026-10-26

### Added

- `enshwick_gamedata::reflect`, the reflection reader that was the
  `enshwick-reflect` crate (#421).
- `Resolved::parts` tells, for each part of a program, which of its
  build table entries a game build lacks (#410).

### Fixed

- A search through a chunk goes on past an entry that panics (#396).

## [1.0.0-alpha.2] - 2026-10-26

No changes of its own. Released with `enshwick`.

## [1.0.0-alpha.1] - 2026-10-26

The first version on crates.io, with the build tables and the game-archive
reader (#347).
