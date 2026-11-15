### Changed

- ABI 12.0, the shape the ABI freezes in; every mod rebuilds for it (#1106):
  - A mod id is only `<author>.<name>` everywhere: manifests, points, packs,
    the join check and item data kinds.
  - `HostApi` is grouped by domain and renamed by one pattern
    (`request_*`, `define_*`, `subscribe_*`, `set_*`); every entry takes the
    mod handle and answers a `Status`, and the C header asserts each offset.
  - New statuses: `Status::LOADER_FAULT` for a fault inside the loader,
    `Status::BUSY` for a full queue or a spent rate and
    `Status::TOO_SMALL` for a short buffer; `MISSING`, `REFUSED` and
    `BAD_ARGS` keep only their own meaning. An unknown enum value or flag
    has one rule, stated in the ABI module.
  - One form each: `Guid` for every GUID, metres as `[f64; 3]` for every
    position and extent (building zone checks included), `u8` 1 or 0 for
    yes and no, `u32` for lengths and counts, and time fields named for
    their unit (`_ns`).
  - A field holding a player's character entity is `character`, a
    `PlayerKey` of an account `account`; struct suffixes name their role
    (`*Data`, `*Info`, `*Spec`, `*Request`).
  - `PlayerInfo` is headed and can grow, keeps a whole name of up to 128
    bytes and has room for further permission bits; `ActionCall` points to
    its `PlayerInfo` and `ActionTarget`, so both can grow.
  - Damage amounts are a pointer and a count sized to the running game's
    damage types (13 on the current build) instead of a fixed array.
  - Lists cross the ABI as a pointer and a `u32` count under one rule, and
    the structs that must grow carry a header.
  - A null callback from a C mod is refused with `Status::BAD_ARGS` instead
    of being called.
  - Every callback and `HostApi` entry states its calling thread, lifetime,
    re-entrancy and panic rules.
  - `ActionCall::SHEET` is gone: `ActionCall::ALLOWED` asks a mod only for
    the ids of the actions a player may run, one per line, and the loader
    chooses carried controls from it.
  - `HostApi::chat` refuses key 0 with `Status::BAD_ARGS`; the new
    `HostApi::chat_all` sends to everyone and `World::broadcast` uses it.
  - Knowledge changes go through one `change_knowledge` with a headed
    `KnowledgeChange`; `run_action` takes its target, `transaction_commit`
    its actor, `set_game_settings` a mask and `change_data` an optional
    change id.
  - `crafting_prop` takes no engine facts from the mod.
  - A resource patch's output lies in a buffer from `resource_buffer`, its
    only form; the doc states which threads run a patch and how long its
    output lives.
  - Survivor lookups and survivor status menus chain across mods in load
    order instead of belonging to the first mod; time speed shows as
    chained.
  - Dropped: `raise_base_limit` as an entry, `BuildTable`, definition texts,
    `ChunkData.index`, the 11.x fallbacks and the commands' name field.
- The world module leaves `unstable`: `World` with its entity and component
  queries, component writes, players, accounts, chat, notices, saves,
  vitals, kicks, bans, chat mutes and join rules, and `Load::world`,
  `Load::on_rename` and `Load::on_join`, in Rust and Luau; props, build
  rights, captures and transactions stay `unstable` (#1106).
- `Arg::template` takes a template from a list the client's window builds
  from its own game and searches as items are; `Value::Template` carries
  it, and the server refuses a template its game does not know before the
  handler runs. Luau `enshwick.actions.template` and
  `enshwick.content.templates` (#1106).
- Loader: an entity call on a build without the world layout fails with its
  own error instead of naming no server (#1106).
- Loader: release builds carry the release signature of the stub and the
  loader; at start the loader checks both off the game thread, and a build
  without a valid signature runs as before while the log (code E1011) and
  the Mods screen name it an unofficial build (#1106).
- `enshwick.h` ships with each release, its comments name C types, and it
  asserts sizes and offsets, the packing and an x64 target; the C++
  manifest has a constant initializer (#1106).
