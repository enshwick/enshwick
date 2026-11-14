# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

## [1.0.0-alpha.15] - 2026-11-14

### Added

- Routes for the client's row frame (`ui.row`), its separator line
  (`ui.separator`, with where it reads its margin, height and offset) and
  the text field's look (`ui.text_field_look`), found on every kept client
  build; the compatibility reports list them (#1023).
- The engine route `server.console_handler`, the routine the dedicated
  server hands `SetConsoleCtrlHandler`, with `server.application` and
  `server.exit_reason`, which it reads, on every kept server build (#1068).
- A route for the word a button's frame reads its kind from
  (`ui.button_kind`), found on every kept client build through the frame
  `ui.button` hands its config to; the compatibility reports list it
  (#1066).

### Changed

- The crate depends on `enshwick-base` and `enshwick-content` instead of
  `enshwick-core`, so it no longer builds every domain of the API (#1049).
- Depends on the internal crate `enshwick-core` instead of `enshwick`; its
  types are the same ones `enshwick` re-exports (#1043).

## [1.0.0-alpha.14] - 2026-11-09

### Added

- A route row bounds its constant with `range = [least, greatest]` and
  `align`, and a `field` route names the type its last field holds with
  `type`; every constant route and every field route of `routes.toml`
  states them, set from the values the kept builds give (#982).
- `Routed::Outside` is a constant a route took outside its range or
  alignment, which finds no entry (#982).
- `reflect::Image::with_text` reads a given copy in place of an image's
  `.text`, such as the executable's code on disk relocated to where the game
  runs, and every other section as before (#1014).
- `releases::newer` finds the newest Enshwick release on GitHub that is
  newer than a running version, by the tag prefixes of a package
  (`LOADER_TAGS`, `TOOL_TAGS`), so the loader and `cargo enshwick` share
  it (#1018).

### Changed

- Breaking: `catalogue::Fact::Offset` names the reflected type its field
  holds (`holds`), and `catalogue::constant` gives no offset for a field
  that holds another type; a route takes an address outside `.text` only
  inside `.data` or `.rdata` (#982).
- Breaking: `catalogue::Fact::Path` is `Path { steps, holds }`, naming the
  type its last field holds; a path offset counts only while each step holds
  the next type inline and the last field holds `holds`, so a step that
  became a pointer or a field that widened leaves it missing (#996).
- Each compatibility report in `reports/` records in `sealed` a digest of
  its text and of the anchors and code that decide it, and
  `cargo xtask check` fails a report whose seal no longer matches, in place
  of the gate on `anchors/` and its `Reports-unchanged` trailer (#1000).

## [1.0.0-alpha.13] - 2026-11-07

### Added

- Engine routes find the interaction prompt's title, valid byte and verb
  and the open screen's host entity in the interface's play data
  (`ui.prompt_title`, `ui.prompt_valid`, `ui.prompt_verb`,
  `ui.screen_host`), the same on every kept client build; the catalogue
  names `SelectedTargets`, `TargetInfo`, `TargetType` and
  `ClientPlayerInput` (#977).
- The archive reader and the catalogue name what a sound and its container
  hold, for added and changed sounds (#977).

- The `test` feature: `Reflection` implements `Default`, an empty
  reflection in which every lookup fails, for tests run without a game
  (#979).
- Engine routes find the entity command buffer's AddComponents and
  RemoveComponents kinds (`ecs.add_components`, `ecs.remove_components`)
  and the RemoveComponents layout (`ecs.remove_components_hashes`,
  `ecs.remove_components_size`) on every game build, the second through
  the engine's own RemoveComponents request (#961).
- `Table::anchors` reads the anchors of a side from the text of any anchors
  file (#962).
- The compatibility reports in `reports/` say what the loader finds on each
  kept build by its anchors and engine routes: the build's name in place of
  a table's, the entries the anchors and routes find, and the attribute
  slots no game archive beside the executable could read as `unread`
  (#962).
- Routes fold a record size or stride from index arithmetic (`take =
  "scale"`), match memory through any base register (`[?+*]`) and take the
  n-th of a known number of runs of a repeated shape after a unique anchor
  (`after`, `nth`, `of`).
  Twelve more constants come from the engine through them, among them
  `ecs.entity_record_size`, `weather.entry_size`, the building box offsets,
  the inventory view copies and the client's `ui.context_bundle` (#951).
- `anchors/patterns.toml` holds, per side, a pattern row without an address
  or build for each entry no route finds, read by `anchors(side)`;
  `reported` says whether a compatibility report covers a build, and
  `Resolved::lack` counts an entry no row holds as missing (#951).
- Routes read a constant from the engine's reflection (`field`, the sum of
  each listed field's offset, found only while each field holds the next
  type inline; `size`, a type's size), take a `spelled` value only where
  a step of a matched two-step or exact shape holds it, and find a
  function as the one direct caller of another whose code the shape
  matches (`callers`). Nineteen constants now come from the engine through
  them (#944).
- Routes find a function through a slot of a routed vtable (`vtable` with
  `slot`), the function the slot's pointer starts, checked by the row's
  `check`. Forty-four more entries now come from the engine: 34 through
  call sites and the functions they reach, among them the client's UI
  state size and no-op constructor, and `net.join_game` with the online
  provider's vtable and the eight join target offsets (#946).
- A table row may name a witness of its value, a second instruction that
  holds the same value (`witness`, `witness_at`, `witness_size`), or say
  why it has none (`one_witness`). A build without its own table takes such
  a value only where the witness is found once and reads the same value.
  Every embedded table has a witness for each value a pattern's operand
  reads and no route finds (#946).

### Changed

- The tables of the kept builds are the frozen record the tests compare the
  routes and anchors with; a new build gets its compatibility report and no
  table (#951).
- Breaking: the crate no longer carries the kept builds' tables or their
  typed catalogues. `kept` reads the tables from the folder `ENSH_GAMEDATA`
  names (`DATA_VAR`, `data_dir`, `tables_in`), an error when it names no
  folder; `embedded` and `typed::SNAPSHOTS` are gone, `typed::snapshot_in`
  names a snapshot in such a folder, `typed::snapshot_path` returns `None`
  without one, and `typed::RENAMES` is the rename table's path in the
  repository. The tests that compare with the tables skip without them, or
  when an executable's build has none (#980).

### Fixed

- The client table of build 26082781 pins the no-op UI constructor and the
  UI state size at the call site the engine route reaches (#946).

### Removed

- The build-table entries no code reads: the ban list rows,
  `server.ban_list_changed` and `server.config`, from every table (#944).
- The `research` anchor class, which no entry has any more (#946).

## [1.0.0-alpha.12] - 2026-11-05

### Added

- The client tables of builds 410, 426 and 520 hold `ui.hud_screen`, the
  interface's current HUD screen, and the catalogue names the player's
  digital input, its `PlayerInputType` and the HUD screen type with its
  `InGame` value (#930).
- `typed`: the covered types of a build as a `Catalogue` read from its
  reflection (the API coverage rule as data in `typed/relevant.toml`), kept
  as a JSON Lines snapshot beside the build tables; `Catalogue::files`
  writes the API crate's `typed` modules and the loader's id table, and
  `Catalogue::diff` classifies what a new build changed, taking a rename
  only on an exact layout match or from `typed/renames.toml`, whose type
  renames resolve under their old ids. `catalogue::type_name` resolves typed
  API ids (#931).
- `Reflection::registry` and `TypeInfo::to_type` hand the API's type
  registry every type with its impact name, flags, element alignment,
  internal hash and the attributes of the type and its fields;
  `Archive::keys`, `contents` and `version` list the archive, keys in the
  order their bytes lie. `reflect::Kind` is the API's `resources::Kind`,
  and `Field` is no longer `Copy` (#925).

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
