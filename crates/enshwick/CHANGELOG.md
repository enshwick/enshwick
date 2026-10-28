# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Added

- The `unstable` feature holds what may still change in any release, so
  1.0 promises only the rest: `world`, `map` with `Load::fog_of_war` and
  the map markers of `ui`, `actions`, `console`, the script binding
  (`script`, `#[derive(Script)]`, `#[script]`), calls (`#[call]`,
  `Load::serve_call`, `Load::caller`, `services::Call` and its kin) with
  the described calls, events and points of script runtimes,
  `Effects::teleport` and `Effects::ping`, the knowledge revokes and
  `Load::on_knowledge_input`, `Load::request_base_limits`, and the world
  edit variants of `WorldError`. Each item says so in its docs, the Luau
  definitions mark `world` and `caller`, and the book's Stability chapter
  names what is stable and how the ABI versions from 1.0.

- `Load::request_base_limits`, behind the new `unstable` feature, asks for
  up to 255 altars a Flame level (ABI 4.1,
  `HostApi::request_base_limits`), and `BaseLimit` tells the game's own
  bases and the most a mod may ask for, with `BaseLimit::past_vanilla`
  (#641).

- `abi::DAMAGE_TYPES` (`ENSHWICK_DAMAGE_TYPES` in C), the length of a hit's
  damage amounts, and `DamageTypeNew::index`, a type's place in them; the
  loader's damage and stats code count by them too (#637).

- `voxels::PER_METRE`, the voxels a metre on every axis, which the
  loader's terrain edits now read too (#632).

- `RecipeKind::ComfortFurniture` (`comfort-furniture` in a data pack, ABI
  code `RecipeKind::COMFORT_FURNITURE`) for placed props that carry the
  item tag of the game's Comfort Furniture crafting group; a prop with the
  Decorative tag too stays `Decorative` (#628).

### Changed

- Breaking: `SkillNode` is `abi::SkillNode` alone; `world` no longer
  re-exports it.
- Breaking: `WorldError` is `enshwick::WorldError`, shared by every module
  that acts on the server's world, and `ComponentType` is
  `ecs::ComponentType`, resolved with `ComponentType::resolve` in place of
  `World::component_type`; `WritableType::resolve` replaces
  `World::writable_type`. `ecs::Components::component` takes a
  `ComponentType` in place of its hash. Inventories, loot and the kill
  credit of `EntityDiedEvent` no longer need the world's reads (#642).
- Breaking: ABI 4.0. Filter inputs and the death record take the game's
  names. `Load::on_craft` hands the game's `CraftRecipeEvent` (`CraftRequest`
  is gone) and `CraftCheck` names its fields `crafting_operator_id`,
  `crafting_station_id`, `recipe_id` and a `u16` `recipe_amount`.
  `PlayerDeathData` has `cause_guid`, `was_deadly_fog` and the causes
  `IMPACT`, `FALL_DAMAGE` and `FOG`. `DamageCause::Fog` and `DeadlyFog`
  (`DamageCheck::FOG`, `DEADLY_FOG`) replace `Shroud` and `ShroudDeath`.
  `BedCheck` names `npc_id` and `bed_id`. The zone check is
  `Load::on_building_zone_check` with `BuildingZoneCheck`
  (`HostApi::building_zone_check_filter`), and the fake host's
  `building_zone_check` (#635).
- A mod whose `init` returns `Err(Off)` hands the loader its reason: the
  Mods screen shows it under the mod and the log names it, an `Off::text`
  in the reader's language.
- Breaking: `Entity` has no serde form, since each load of a world numbers
  its entities anew; `Prop` has none either, and a `Capture` keeps
  `CapturedProp`, the item and transform without the entity. Entity data
  refuses an entity no load finds again, a player's character, with
  `SetError::NotKept` (`Status::NOT_KEPT`, ABI 3.5) (#631).
- Entity and item data keep each value under its entity's place, the
  template and position as the save keeps them or the container and slot,
  and find it again after a load; a value no place names for certain goes
  aside in the world's file and never attaches again (#631).

### Fixed

- With the base limit raised, every altar zone the game lists joins its
  base: the game copied only the first 16 into its bases, so zones past
  16 had no comfort, crafting from chests or survivors (#641).
- "Remove water" on a Flame Altar clears every zone of its base, not only
  the first 16 (#641).
- After a restart that followed any added or removed entity, an entity or
  item data value could land on another entity of the same template; values
  now follow their entity's place. Values of a file written before this
  stay aside, attached to nothing (#631).
- Item data on a leaving player's items goes aside in the world's file
  instead of being dropped with the items the game removes, and a place
  keys exactly as the save keeps it, a negative axis included (#640).

## [1.0.0-alpha.10] - 2026-10-28

### Added

- `Off::text(key, args)` turns a feature off with a text of the mod's
  texts: the Mods screen, the status summary and the log show it in the
  reader's language. A `ConfigError::text` turned into an `Off` stays a
  text (#630).
- `Hold::set_text(reason, key, args)` sets a hold whose reason the log
  shows in the reader's language while the mod's texts are loaded, and as
  `reason` when they are not; a hold file of one line reads as before
  (#630).
- `BuffTypeId` and `NpcId`, the game's id of a survivor or other NPC, are
  ABI ids with a `Shape` (ABI 3.4); they print as their number, points and
  events carry them, and a Luau script reads and answers them as
  `buff_type_id` and `npc_id`. Every id the game derives from a GUID has
  `named` (#623).
- `loot::LootContainerId`, the game's name for a loot roll's node and
  labels (#623).
- `RecipeKind::Decorative` (`decorative` in a data pack, ABI code
  `RecipeKind::DECORATIVE`) for placed props that carry the item tag of
  the game's Decorative crafting group, so `Recipes::Kind`,
  `Rate::RecipeOutput` and `RecipeView::kind` pick decorations by the
  game's own grouping (#616).
- The title screen shows the loader's line under the game's version,
  `Enshwick 1.0.0-alpha.9 · mods: 6`, with the mods off and a warning when
  the game version leaves features off (#624).
- `Language::OFFERED`, the languages the game's settings offer, and
  `Texts::missing_in(languages)`, each language's missing texts, so a mod's
  test holds its texts complete for every offered language (#618).
- Every id type prints as its number (`Display`): `Entity`, `ItemId`,
  `PlayerKey`, `RecipeId`, `AffixId`, `SkillNode` and `KnowledgeId` go
  into a log line as they are (#615).
- `KnowledgeId` names a knowledge value: a recipe's, a quest's or any
  other the world or a player keeps; a `RecipeId` converts into it
  (ABI 3.2) (#615).
- `ItemId::from_guid`, `RecipeId::from_guid` and `KnowledgeId::from_guid`
  give the game's id of a GUID with its type, in a constant too (#615).
- `KnowledgeId` and `SkillNode` have a `Shape`, so services, events and
  points carry them; a Luau script changes an answer of any id type
  (#619).
- `Load::on_tick` runs a tick handler on a period of wall time and returns
  a `TickPeriod`, whose `set_every` changes the period while the game runs;
  `Load::on_every_step` runs one on every server step. A step a handler
  skips never enters the mod (`HostApi::subscribe_tick`,
  `HostApi::set_tick_every`, `TickHandle`, ABI 3.3) (#614).
- `TickData` carries the steps and the time since the handler's last call,
  the average step interval and, with timed diagnostics, the mod's recent
  cost per handler call: `elapsed_steps`, `elapsed`, `step_interval` and
  `cost`, with `TickData::new` and `with_elapsed` for tests (ABI 3.3) (#614).
- The fake host runs steps as the loader does with `Installed::tick` (#614).
- `Map::frame` returns a `MapFrame`: where the scene's map starts, how far
  it reaches and the size of its cells, in metres, read from the game's
  data once per scene like the markers (ABI 3.4, `HostApi::map_frame` and
  `MapFrameInfo`); the fake host serves one with `FakeHost::map_frame`
  (#627).

### Changed

- Breaking: `survivors::SurvivorId` is `NpcId`; the buff and survivor ABI
  structs, `HostApi::remove_buff`, `summon_item` and the survivor status
  merge take `BuffTypeId` and `NpcId`, and `BuffTypeId::from_guid` takes a
  `Guid`. The fake host's `summon_items` and `survivor_status` carry the
  ids (#623).
- Breaking: `Roll::node` and `Roll::labels` are `Option<LootContainerId>`,
  `None` where the game has none, and `Source::nodes`, `labels`,
  `with_nodes` and `with_labels` take `LootContainerId` (#623).

- Breaking: `Load::on::<Ticked>` runs the handler every
  `Ticked::DEFAULT_EVERY` (100 ms) instead of on every step; so do Luau's
  `enshwick.on("Ticked", ...)` and the scheduler, whose `next_tick` runs on
  its next pass, within that period. `Load::on_every_step` keeps the old
  timing (#614).
- Breaking: `StepClock::step(instant)` is `StepClock::advance(elapsed,
  steps)`, and the mod's step clock moves by the time its ticks carry,
  without reading the clock itself (#614).
- The loader reads the clock once per server step and hands that reading
  to every part of the step that needs the time (#614).
- `abi::read_headed` copies a struct in one move when the writer's struct
  covers all of it, so a larger event struct costs a handler no more (#614).
- Breaking: `MapSnapshot` carries the map's `frame`, and
  `MAP_SNAPSHOT_FORMAT` is 2 (#627).
- Breaking: `Guid::id` is `Guid::fnv1a`, the bare hash, and `Id::id` is
  gone; `ItemId::from_guid(id.guid())` and its siblings take their place.
  `World::used_item` returns an `ItemId`, and `migrations::current_id`
  takes and returns one (#615).
- Breaking: the knowledge calls, `KnowledgeChanged`, `QuestCompleted` and
  their ABI structs take `KnowledgeId`; `Knowledge::quests` returns them.
  `SkillNode` is an ABI type, and `abi::SkillCheck::node` and
  `HostApi::skill_override` take it (#615).
- Breaking: the fake host's `survivor_lookup` takes a `SurvivorId` and
  `Option<Entity>` for no entity and returns the mod's `Answer`;
  `set_knowledge`, `grants`, `revokes` and `account_changes` carry
  `KnowledgeId`, and `None` for the world (#615).
- A text missing in a regional language comes from its main variety before
  English: `pt-br` from `pt`, `es-mx` from `es-419` and `es`, `fr-ca` from
  `fr`, `nb` from `no` (`Language::lookup_order`) (#618).
- The loader's own texts sit in a file per language, and every console
  answer, the status summary, the per-mod costs, the conflict report and
  the interface demo follow the reader's language (#618).
- A player's game tells the server its language when it joins; the
  loader's answers, join notes and mod names to that player follow it, and
  on a server `i18n::language()` is that player's language while the loader
  asks a mod about the player. The loader's setting `language` sets the
  language of its console. The join message protocol moved to version 2
  (#620).
- `ConfigError::text(key, args)` refuses settings with a text of the mod's
  texts, which each player reads in their own language; the `validate` of
  the `Config` derive takes any error that converts into `ConfigError`
  (`String`, `&str` or a `ConfigError`) (#620).
- The Mods screen scrolls its lists in the game's own scroll area instead
  of paging them, and a text setting takes typed text in the game's own
  text field, set on Enter. A game build without either keeps the pages
  and the read-only line, and the log names what is missing (#625).

### Removed

- Breaking: `Load::modify_recipes`, `RecipeEdit` and `modify_recipes_in`;
  a data change through `Load::change_recipes` or a `Rate::RecipeOutput`
  changes recipe counts by kind (#616).

### Fixed

- The Mods screen shows a secret setting as "(secret)" and never its value
  (#626).

## [1.0.0-alpha.9] - 2026-10-27

### Added

- `code::find_each_unique` finds many patterns in one pass over the code,
  each with the result `Pattern::find_unique` gives (#598).
- `World::account_action` asks the game to kick, ban or unban an account
  (`AccountAction`); a ban of an account that is not online goes straight
  onto the server's ban list (`HostApi::account_action`,
  `AccountActionType`). The fake keeps a ban list
  (`Installed::banned`, `removed`) (#586).
- With `allowlist = true` in the loader's settings, a dedicated server lets
  in only the accounts `mod-data/enshwick/allowlist.txt` lists, and its
  administrators; the console's `enshwick allowlist add|remove` and
  `World::set_allowlisted` change the list, `World::allowlisted` reads it
  (`HostApi::allowlist_set`, `HostApi::allowlist_has`, ABI 3.1). The
  fake keeps the list (`Installed::allowlisted`) (#602).
- `error_once!` and `info_every!`, so every level that has a once or a
  periodic macro has both (#597).

### Changed

- `World::account_action` refuses a kick or a ban of a hosted game's host,
  succeeds for a ban of an account on the ban list already, and refuses a
  ban the game's list does not take instead of answering that the action
  is missing; the console says so in its own words. The fake takes the
  host with `FakeHost::host_key` (#606).
- The crate docs say to depend on the crate from crates.io and carry a
  module map, the feature table and the target; docs.rs builds every
  feature. The Luau definitions describe events, codes and permission bits
  in plain words, without Rust paths (#604).
- Breaking: the ABI is 3.0. A mod built for ABI 2 is refused and needs a
  rebuild against this release; the C header's `ENSHWICK_ABI_MAJOR` is 3 (#599).
- Breaking: ids have their own types across the ABI and the safe API.
  `Entity`, `ItemId`, `PlayerKey` and `RecipeId` live in the ABI module,
  re-exported from the crate root, and the C header declares them as
  `EnshwickEntity`, `EnshwickItemId`, `EnshwickPlayerKey` and
  `EnshwickRecipeId`; `ecs::Entity` is now `enshwick::Entity`. Account keys
  (`Player::key`, `Peer::Player`, actions' player lists,
  `Knowledge::grant_account`, `World::account_action` and the allowlist),
  item ids (inventory ops and slots, drops, interactions, vetoes, summons,
  `ItemInstance`, terrain materials) and recipe ids (`CraftRequest`,
  `CraftRecipeEvent`) take them, and the fake host's methods do too.
  `affixes::AffixId` names an affix in this process (`Load::define_affix`,
  `Affixes::info`, `Line::affix`), and `world::SkillNode` a skill node
  (`Skill::node`). `Summon::ammo` is an `Option`. Point and event schemas
  name the four ABI ids as built-in types, so `cargo enshwick point` writes
  them as the API's types (#608).
- Breaking: `Load::on` subscribes to typed game events too, as Luau's
  `enshwick.on` does: `load.on::<EntityDiedEvent>(..)` replaces
  `load.on_game`. `events::Kind` is the one trait `on` takes; the loader
  events' `EVENT` constant and raw read moved to `events::LoaderEvent`,
  which `FakeHost::raise` takes. `game_events::Kind` is gone (#597).
- Breaking: `Load::event_type` and `Load::event_types` are
  `game_event_type` and `game_event_types`, the names Luau uses;
  `FakeHost::event_type` is `game_event_type` (#597).
- Breaking: `Load::on_skill` takes the `Skill` and finds its node itself;
  `Load::skill_node` and `SkillCheck::node` are gone, and `SkillCheck` is
  `#[non_exhaustive]` (#597).
- Breaking: `Load::buff_modifiers` is `add_buff_modifiers` (#597).
- Breaking: `mods::all()` lists the loaded mods; the crate root's
  `enshwick::mods()` and `Host::{mods, join_missing, replaced}` are gone,
  so each list has one call: `mods::all`, `mods::join_missing`,
  `mods::replaced`. `mods::label` is hidden: `ModEntry::label` names a mod (#597).
- Breaking: `ReadPackError::TooLarge` names the `limit` the file broke, and
  `PackFiles::read_dir` refuses a pack with more than `MAX_ICONS` icons
  (`ReadPackError::TooManyIcons`) or an icon over `MAX_ICON_FILE`; icon
  PNGs past `icons::MAX_SIDE` pixels do not decode (#596).
- `mods::join_missing` leaves out an entry with new text once the loader
  keeps 64 KiB of text servers advertised (#596).
- `declare_mod!` exports the five functions under their C names only
  outside test builds, so one test binary can declare several mods (#591).

### Fixed

- A server rates the settings queries, setting changes and actions queries
  of a player who is no administrator, 2 a second in bursts of 8 (#596).
- Settings errors name the key and the expected type, and the line where
  the text is at hand, never the value: the settings check's reason, the
  settings read at start, table settings, join values, migrations, and
  `ConfigError` and `MigrationError` made from a TOML error. The warning for
  a value held to its range no longer shows the old value (#594, #595).
- `Schema::parse` refuses a field or group key with an empty part, such as
  `auth..token`, and `Load::table_config` a field with an empty key; the
  loader refuses such a schema, and the mod's warning names the key (#593).

### Removed

- Breaking: the retired `HostApi` entries `retired_command`,
  `altar_zones_control`, `set_altar_zones` and `altar_limit`, with
  `AltarZoneSize` and `AltarLimitFn`; every later entry moves up. The ABI
  docs no longer say in which minor of ABI 2 a field came (#599).
- Breaking: `Load::settings`, which read the settings file into any
  `Deserialize` type past the stages, `sync` and `secret` checks: derive
  `Config` and call `Load::config` (#597).

## [1.0.0-alpha.8] - 2026-10-27

### Added

- `packs::script_files` lists the scripts `PackFiles::read_dir` would read,
  without reading them. The loader lists a Luau mod's scripts each time it
  looks, so a script added, removed or renamed while the game runs reloads
  the mod with `dev_reload` as an edited one does (#585).
- A Luau mod reads and changes inventories through `enshwick.inventory`:
  `slots`, `count`, `item_info`, `give`, `take`, `destroy_slot`,
  `move_slot` and `move_all`, the done function getting the `Outcome` as a
  table with `Done` and `OpError` by name. `Inventories::slots` and
  `Inventories::count` read an inventory on the server's game thread
  without a `Reader` (#581).
- A Luau mod calls another mod's call with `enshwick.caller(description)`,
  from the module `cargo enshwick call --luau` writes; the input's fields
  are numbers, booleans, entities and GUIDs, each checked against its
  layout hash (#577).
- Calls, described services: `#[call]` declares one function from a
  `repr(C)` input to an answer, `Load::serve_call` serves it and
  `Load::caller` gives a `Caller` that runs it from any thread
  (`services::Call`, `CallError`); `manifest!(calls: [..])` puts its schema
  into the manifest. `DescribedCall`, `Load::described_caller` and
  `Load::serve_described_call` do the same with bytes, for a runtime that
  knows a call from its schema. The loader matches callers by name, major,
  fields and sizes once every mod is initialised (`HostApi::serve_call`,
  `find_call`, `call`, `CallSpec`, ABI minor 95). The fake serves and calls
  them (`Installed::serve_call`, `call`, `served_calls`). A call's input
  and answer have no padding, which `Shape::PADDING_FREE` reports (#574).
- `Load::on_described_mod_event` and `Load::define_described_event`
  (`services::DescribedEvent`, `DescribedEmitter`) hear and send a mod
  event described at run time, its data as bytes in the definer's layout;
  `manifest!(events: [..])` puts the schema of each listed `#[event]` into
  the manifest, and `ModEvent::SCHEMA` and `DESCRIBED` carry it (#571).
- `load.fog_of_war()` returns the client's `FogOfWar`, which the loader
  copies as the game uncovers it: `FogOfWar::state`, `read` and `coverage`
  read what the player has uncovered, as a `FogState` and `FogImage`; the
  `fog_open` and `fog_read` entries and `FogInfo` (ABI 2.94) (#564).
- `Change::in_table` changes an item's count, chance or weight in one fill
  table alone, after its changes of every table; `Vanilla::loot_table`
  finds the table a container template rolls, with how many containers
  share it; a data pack's `loot.toml` takes `table = [...]` with the
  containers' template names. `FillTableLayout` and `template_table` read
  a template's name and table (#570).
- `Hold::for_world` holds the game for one world only
  (`mod-data/<id>/worlds/<key>/hold.txt`): the loader stops the game as
  that world starts to load without the mod, before its save is read, and
  on a build it leaves vanilla at start; `world_holds`, `held_worlds` and
  `WORLDS_DIR` read them. A data pack that raised a stack size now holds
  the worlds saved with it, not the whole data folder (#563).
- `Field::Station` and `Change::station` move a vanilla recipe to another
  workshop (`DataField::RECIPE_STATION`, ABI minor 93): the loader writes
  the recipe's workshop and lists it in the client's crafting menu at the
  new station alone, from the next start; a data pack's `recipes.toml`
  takes `station = "<station item>"`. `menu::move_recipes` and
  `recipe_workshops` do the same for a mod's own patches (#567).
- `Items::Backpacks` selects every backpack for a change of `Field::Slots`,
  which now reaches a backpack's own inventory too, for backpacks made
  after it (`DataSelect::BACKPACKS`, ABI minor 92); a data pack takes
  `items = "backpacks"`, and `ItemTraits::backpack_slots` reads a
  backpack's size (#566).
- `Load::extend_described_point` links into an extension point described at
  run time (`points::DescribedPoint`, `DescribedField`), the input and the
  answer as bytes in the owner's layout, as a script's schema gives them;
  the loader checks the layout hashes as for a typed link (#569).
- `Load::change_recipes` changes the output count, cost or time of every
  vanilla recipe a selection picks: `Recipes::Kind` by what the recipe
  makes, `Recipes::Workshop` by where it is made, read from the game's
  data by the loader (`DataSelect::RECIPE_KIND`, `DataSelect::WORKSHOP`,
  ABI minor 91); a data pack's `recipes.toml` takes `kind = [...]` and
  `workshop = [...]` for the same, and `RecipeKind::name` gives the names
  (#562).
- `Map::snapshot` takes the map as a `MapSnapshot` of players, Flame Altars
  with their bases, and markers, plain data for a process outside the game;
  `MapSnapshot::to_bytes` and `from_bytes` write and read it in a versioned
  format (`MAP_SNAPSHOT_FORMAT`) (#560).
- `script::game_events` lists every typed game event for script languages
  (`GameEventKind`); the game event structs, masks and values convert for
  scripts (a mask as its bit names, a value as its name) (#565).
- `load.map()` returns the `Map` of the game server: `Map::markers` and
  `Map::markers_in` list the world's map markers, each a `MapMarker` with
  its position, its `MapMarkerTypeId` (the key of the game's marker type
  registry) and its label's text tag; `Map::waypoint` reads the waypoint a
  player set (#557).
- The `world_quiet` entry answers whether a mod is on the server's game
  thread at a quiet moment while a world runs, where it may read live
  components (ABI 2.90) (#557).
- `Load::table_config` reads settings described by a `config::Schema` at
  run time, with the file, checks, stages, Mods screen and join values of a
  typed `Config`; `config::TableConfig::on_change` follows them, and
  `Schema::template` writes the file (#561).
- A pack's `pack.toml` declares settings in `[[setting]]` tables
  (`PackSetting`, `PackManifest::settings_schema`), checked as it is read (#561).
- `World::can_build` tells whether the server lets a player edit a box:
  `CanEditWorld`, and `CanEditBase` inside a building zone, as the game's
  terrain tools decide, plus two tests from the game's building cursor, the
  world's building area and strict no-build zones; a refusal is a
  `BuildRefusal` with the game's `CursorFlag` names (#547).
- `World::transaction` writes terrain edits as one transaction at the
  server's quiet moments, 64 bricks a step, and `World::undo` puts every
  brick it touched back, refusing when one has changed since;
  `World::transaction_status` follows it (#548).
- `WorldError::OutsideWorld` (`Status::OUTSIDE_WORLD`): voxel reads, terrain
  edits and transactions past the world's edge or above 1,024 metres are
  refused, where the game would read air (#548).
- `World::props_in` lists the props in a box, the things players placed
  from items, each with its `ItemId` and `WorldTransform`, told apart from
  the things a world comes with by the template their item places (#550).
- `World::capture` takes a region of up to `CAPTURE_TILES` reads of 32
  metres a side as a `Capture` of its voxels and props, counted from its
  corner; `Capture::rotated` turns it by quarter turns about the vertical
  and `Capture::translated` moves it (#550).
- `ItemId`, the game's number for an item, and `WorldTransform`, a
  position in metres with an orientation (#550).
- `Capture`, `VoxelBox`, `Voxel`, `VoxelMaterial`, `VoxelPos`, `Entity`,
  `ItemId`, `Prop` and `WorldTransform` implement serde's `Serialize` and
  `Deserialize`; a `VoxelBox` read must hold one voxel per place and end
  inside the world's coordinates (#550).
- `ItemData::placed_entity`, the GUID of the template a player places from
  the item, from `HostApi::item_info`; ABI minor 88 (#550).
- `Transaction::write_voxels`, `erase_block` and `place_block` write voxels,
  tear down building voxels and place building blocks inside a
  transaction, with the same undo; blocks and tear-downs raise the game's
  `BuildingPlaceEvent` and `BuildingTearDownEvent`. `VoxelBox::new`,
  `set` and `moved_to` build boxes to write (#551).
- `World::transaction_for` acts for a player: the loader checks the
  player's rights for every change before it writes, and a refusal shows
  as `TransactionStatus::NotAllowed`; the op kinds, records and
  `transaction_commit_for` are ABI minor 89 (#551).
- The conflict report lists code patches refused over another mod's
  bytes, vanilla values one mod's set overrides with both values, and
  components several mods write; the loader compares the game's code with
  the executable on disk while it starts and names the DLL or mod that
  changed it outside the API, after each load, `init` and `ready` (#555).
- A data pack's `items.toml` changes the stack size or container slots of
  every vanilla item of some categories (`category = ["Materials"]`), every
  item that stacks (`items = "stackable"`) or every container
  (`items = "containers"`), through the selections `Load::change_items`
  makes; `Pack::selections` lists them, and `ItemCategoryMask::NAMES` and
  `ItemCategoryMask::named` take the game's category names (#558).
- `Status::STATIC_HALF` (ABI minor 94), `WorldError::StaticHalf` and
  `FakeHost::static_half`: a write to the static half of a component
  type, the set-up data every entity of a template shares, is refused
  (#568).
- `Transaction::place_prop`, `remove_prop` and
  `remove_prop_discarding_state` place and remove props inside a
  transaction, with undo; `World::transaction_entities` lists the props a
  transaction placed. A prop that holds state a player would lose (items,
  growth, a production queue, assigned survivors, written text, fuel,
  logic wiring, a logic trigger) is never
  removed or taken away by an undo unless the mod opts in:
  `WorldError::HoldsState(PropState)` names what it holds. The op kinds,
  records and `transaction_entities` are ABI minor 96 (#552).
- `World::persists` tells whether a save keeps changes in a box: the
  game saves a world only inside its bases' zones, so props, blocks and
  terrain changed elsewhere are gone after a restart, a mod's as a
  player's. `HostApi::world_persists` is ABI minor 96 (#552).
- `WorldError::NotAProp` (`Status::NOT_A_PROP`, ABI minor 96): creatures
  placed from items (animals, pets, survivors) and flame altars are no
  props; prop removal refuses them in either mode (#552).

### Changed

- `#[point]` and `#[event]` refuse an input, answer or data with padding,
  as `#[call]` does, since a described link or listener reads every byte:
  add the bytes as fields. The example trading post's `price` gains a
  `count` (#587).
- `Map::markers` reads the markers the scene places from the game's data
  through the new `map_markers` entry and `MapMarkerInfo` (ABI 2.97), once
  per scene; `MapMarker` no longer names an entity (#573).
- An item a mod or pack changes by name keeps that change over its own
  selection of the same field, so a selection can leave out exceptions
  (#558).
- `Terrain::{add,dig,flatten}` write nothing and fail with
  `WorldError::NotLoaded` unless the whole area the edit can reach is
  loaded; the game would compute such an edit against air and merge it
  onto the ground once the area loads (#548).
- `World::entities` takes the world by value, as the other `World` calls
  do (#550).
- Item data (`Inventories::item_info`, `HostApi::item_info`) and the
  item checks of `World::props_in` and `Transaction::place_block` work from
  world start: the loader finds the item registry without waiting for a
  player to join (#551).
- `World::props_in` and `World::capture` leave out creatures placed from
  items (animals, pets, survivors) and flame altars, which are no props,
  and answer `WorldError::Missing` on a game build without their marks
  (#552).

### Fixed

- A Luau mod's answer to a point and its mod events take an enum over a
  64-bit integer, `usize` and `isize` included, as a number both ways;
  before, every such value was refused (#587).
- The loader's profile names the survivors feature `survivors`; it showed a
  mod id (#574).
- The loader and the fake leave a mod event subscription idle when it
  reads more bytes than the definer sends (#571).
- `Map::markers` listed no marker on a dedicated server, whose marker
  entities the entity walk does not find (#573).
- `World::component` and `World::get` read the static half of a component
  type, such as `FuelSetup`, as the template's set-up values; they gave the
  data of its dynamic half, `DynamicFuelSetup`, before (#568).

## [1.0.0-alpha.7] - 2026-10-26

### Added

- `load.voxels()` reads up to 64³ voxels of the server's world with
  `Voxels::read` and `Voxels::sample`, on the game thread at a quiet
  moment, as `VoxelBox` of `Voxel`s; `Status::NOT_LOADED` and
  `WorldError::NotLoaded` name a part of the world that is not loaded
  (#541).
- `Load::change_items` changes a stack size or container slots of every
  vanilla item a selection picks (`Items::Category`, `Items::Stackable`,
  `Items::Containers`), read from the game's data by the loader;
  `ItemCategoryMask` names every item category (#540).
- `Load::declare_change` declares a change of a vanilla value at init whose
  value the mod sets any time later through its `ChangeHandle`
  (`set`, `reset`); the loader merges it with every mod's and pack's
  changes and builds what it reaches again in place, so a stack size or
  chest size can follow a per-world setting (#542).
- The loader keeps every stack size above the game's that a world was saved
  with (`mod-data/enshwick/kept-stacks.toml`) and serves at least that size
  from the start, so lowering a size or removing the mod that raised it
  never cuts a saved stack; while any size is kept it holds the game on a
  build it would leave vanilla (#544).
- `script` describes the API for script languages beside the Rust items:
  `#[derive(Script)]` for types, `#[script(<part>)]` for the methods a script
  may call, a script entry for every loader event, and `NAMESPACES` (#543).
- A data pack folder with a `main.luau` reads its scripts:
  `PackFiles::scripts`, `SCRIPT_MAIN`, `SCRIPT_EXTENSION`,
  `DEFINITIONS_SUFFIX`, `MAX_SCRIPTS`, `MAX_SCRIPT_DEPTH` and
  `ReadPackError::TooManyScripts`; scripts count in the pack's hash (#545).

### Fixed

- Each mod handle has a scheduler queue of its own, so one copy of the API
  can run tasks for several packs and scripts (#543).

### Changed

- `ItemCategoryMask` lives in `content::categories`, beside the selections
  that use it, instead of `affixes` (#540).

## [1.0.0-alpha.6] - 2026-10-26

### Added

- `content::loot_tables` reads and changes the random loot table's stack
  sizes and every stack of a fill table; a data pack's `loot.toml` changes
  `stack_size`, `count`, `spawn_probability` and `weight` under the game's
  own names (#523).
- `altars::base_limit` names the most altar zones and bases the game groups
  now, and the zones its own grouping takes (#529).
- `ContainerKinds`, `ItemCategoryMask` and the event masks have a
  `const fn union`, so a combined set can be a `const`; `|` gives the same
  (#539).

### Changed

- The altar limit is a chain any mod joins: `Load::on_altar_limit` takes a
  filter that sees the Flame level, the game's own count and the count so
  far in an `AltarLimit` and changes `allowed`, in load order. The loader
  keeps every answer within the zones the game groups, and owns the
  raised maximum of 32 zones in 16 bases; `Load::raise_base_limit` puts it
  in force from `Mod::ready` on (#529).
- `World` is one pointer to what the mod resolved once, instead of 320
  bytes: a copy is cheap, and passing it by value or moving it into a
  closure no longer trips `clippy::large_types_passed_by_value`. Its
  methods take `self`, so calls read the same (#536).

### Removed

- `VANILLA_ALTARS_PER_FLAME_LEVEL`, `vanilla_altars`, `VANILLA_ALTAR_ZONES`
  and `RAISED_ALTAR_ZONES`: a filter reads the game's own count from
  `AltarLimit::vanilla`, and `altars::base_limit` names the zones (#529).

## [1.0.0-alpha.5] - 2026-10-26

### Added

- `Rate::RecipeOutput(kind)` multiplies what the recipes of each
  `RecipeKind` make, after every mod's own recipe patch;
  `RecipeEdit::scale_output_by` scales by a fraction and
  `RecipeKind::ALL` lists the kinds (#501).
- `progress::Skill` and `Load::skill_node` give the node of a skill the
  platform names on the running game build (#494).
- `game_events::EquipmentSlot`, the slot `EquipmentChanged` carries, and
  `inventory::OpError`, why an inventory operation failed, both named as
  the game names them (#494).

### Changed

- Altar zone sizes are a chain any mod joins: `Load::on_altar_zone` adds a
  filter that sees each altar's zone so far, the vanilla one first, in load
  order, and `ZoneSizes::changed` asks for a new sizing from any thread
  (#499).
- The loader owns the event loop, server loop and task worker hooks:
  `on_loop_wait`, `on_server_wait` and `cap_task_workers` keep their
  signatures but run through the host, handlers of several mods run in load
  order and the lowest worker cap wins; a fake host test runs them with
  `Installed::loop_wait`, `server_wait` and `task_workers`, no longer
  through `mid_hooks` or `run_at` (#486).
- Game settings, hit damage types and attack categories, experience
  sources, altar actions, equipment slots and inventory error codes resolve
  through engine ids and the game's constant names; no engine number is
  compiled into a mod (#494).
- `DamageTypes` and `AttackCategories` keep their bits private and combine
  with `|`; `EquipmentChanged::slot` is an `EquipmentSlot` and
  `OpResult::error` an `Option<OpError>` (#494).
- `World::writable_type` accepts the copies the game keeps for clients; a
  write to one is refused with `WorldError::Refused` (#494).
- Names of things the game names itself are the game's reflected names in
  Rust casing; the game settings enums read their values by the game's
  constant names (#506):

| Old | New |
| --- | --- |
| `game_events::DamageTypes` | `game_events::DamageTypeMask` |
| `DamageTypes::CUT, PIERCE, BLUNT` | `DamageTypeMask::PHYSICAL_CUT, PHYSICAL_PIERCE, PHYSICAL_BLUNT` |
| `DamageTypes::FIRE, ICE, SHROUD, LIGHTNING` | `DamageTypeMask::MAGICAL_FIRE, MAGICAL_ICE, MAGICAL_FOG, MAGICAL_LIGHTNING` |
| `game_events::AttackCategories` | `game_events::AttackCategoryMask` |
| `AttackCategories::AREA` | `AttackCategoryMask::AOE` |
| `game_events::ExperienceSource` | `game_events::ExperienceSourceType` |
| `game_events::AltarAction` | `game_events::FlameAltarActionType` |
| `AltarAction::SetSpawn` | `FlameAltarActionType::SetAsPreferredSpawnAltar` |
| `game_settings::Preset` | `game_settings::GameSettingsPreset` |
| `game_settings::Difficulty` | `game_settings::GenericDifficulty` |
| `game_settings::Amount` (`random_spawns`) | `game_settings::RandomSpawnerAmount` |
| `game_settings::Amount` (`aggro_pool`) | `game_settings::AggroPoolAmount` |
| `game_settings::StartleRepercussion` | `game_settings::TamingStartleRepercussion` |
| `Settings::player_health` | `Settings::player_health_factor` |
| `Settings::player_mana` | `Settings::player_mana_factor` |
| `Settings::player_stamina` | `Settings::player_stamina_factor` |
| `Settings::player_body_heat` | `Settings::player_body_heat_factor` |
| `Settings::player_diving_time` | `Settings::player_diving_time_factor` |
| `Settings::durability` | `Settings::enable_durability` |
| `Settings::starving` | `Settings::enable_starving_debuff` |
| `Settings::food_buff_duration` | `Settings::food_buff_duration_factor` |
| `Settings::hunger_to_starving` | `Settings::from_hunger_to_starving` |
| `Settings::shroud_time` | `Settings::shroud_time_factor` |
| `Settings::tombstone` | `Settings::tombstone_mode` |
| `Settings::glider_turbulence` | `Settings::enable_glider_turbulences` |
| `Settings::weather` | `Settings::weather_frequency` |
| `Settings::fishing` | `Settings::fishing_difficulty` |
| `Settings::mining_damage` | `Settings::mining_damage_factor` |
| `Settings::plant_growth` | `Settings::plant_growth_speed_factor` |
| `Settings::resource_drops` | `Settings::resource_drop_stack_amount_factor` |
| `Settings::factory_speed` | `Settings::factory_production_speed_factor` |
| `Settings::perk_recycling` | `Settings::perk_upgrade_recycling_factor` |
| `Settings::perk_cost` | `Settings::perk_cost_factor` |
| `Settings::experience_combat` | `Settings::experience_combat_factor` |
| `Settings::experience_mining` | `Settings::experience_mining_factor` |
| `Settings::experience_exploration` | `Settings::experience_exploration_quests_factor` |
| `Settings::random_spawns` | `Settings::random_spawner_amount` |
| `Settings::aggro_pool` | `Settings::aggro_pool_amount` |
| `Settings::enemy_damage` | `Settings::enemy_damage_factor` |
| `Settings::enemy_health` | `Settings::enemy_health_factor` |
| `Settings::enemy_stamina` | `Settings::enemy_stamina_factor` |
| `Settings::enemy_perception` | `Settings::enemy_perception_range_factor` |
| `Settings::boss_damage` | `Settings::boss_damage_factor` |
| `Settings::boss_health` | `Settings::boss_health_factor` |
| `Settings::pacify_enemies` | `Settings::pacify_all_enemies` |
| `Settings::startle` | `Settings::taming_startle_repercussion` |
| `Settings::day_length` | `Settings::day_time_duration` |
| `Settings::night_length` | `Settings::night_time_duration` |
| `Settings::curse` | `Settings::curse_modifier` |
- Typed game events and their fields take the game's reflected names;
  `Block`, `killer` and `killer_player` stay enshwick's (#518):

| Old | New |
| --- | --- |
| `game_events::Hit` | `game_events::HitEvent` |
| `Hit::attacker` | `HitEvent::root_source_id` |
| `Hit::source` | `HitEvent::source_id` |
| `Hit::target` | `HitEvent::target_id` |
| `Hit::root_target` | `HitEvent::root_target_id` |
| `Hit::weapon` | `HitEvent::weapon_pide_id` |
| `Hit::attack` | `HitEvent::attack_category` |
| `game_events::Died` | `game_events::EntityDiedEvent` |
| `Died::entity` | `EntityDiedEvent::dead_entity_id` |
| `game_events::LevelGained` | `game_events::LevelUpEvent` |
| `LevelGained::player` | `LevelUpEvent::player_entity_id` |
| `game_events::ExperienceGained` | `game_events::XpGainedEvent` |
| `ExperienceGained::player` | `XpGainedEvent::player_entity_id` |
| `game_events::CraftRequested` | `game_events::CraftRecipeEvent` |
| `CraftRequested::player` | `CraftRecipeEvent::crafting_operator_id` |
| `CraftRequested::station` | `CraftRecipeEvent::crafting_station_id` |
| `CraftRequested::recipe` | `CraftRecipeEvent::recipe_id` |
| `CraftRequested::amount` | `CraftRecipeEvent::recipe_amount` |
| `game_events::Crafted` | `game_events::CraftRecipeSuccessEvent` |
| `Crafted::workshop` | `CraftRecipeSuccessEvent::workshop_id` |
| `Crafted::recipe` | `CraftRecipeSuccessEvent::recipe_id` |
| `game_events::Interacted` | `game_events::AcceptOfferedInteractionEvent` |
| `Interacted::player` | `AcceptOfferedInteractionEvent::guest_entity_id` |
| `Interacted::target` | `AcceptOfferedInteractionEvent::host_entity_id` |
| `Interacted::offer` | `AcceptOfferedInteractionEvent::offer_id` |
| `game_events::Built` | `game_events::BuildingPlaceEvent` |
| `Built::item` | `BuildingPlaceEvent::tracking_item_id` |
| `game_events::TornDown` | `game_events::BuildingTearDownEvent` |
| `game_events::PickedUp` | `game_events::BuildingPickUpEvent` |
| `PickedUp::entity` | `BuildingPickUpEvent::picked_entity_id` |
| `game_events::AltarUsed` | `game_events::FlameAltarEvent` |
| `AltarUsed::player` | `FlameAltarEvent::player_entity_id` |
| `AltarUsed::altar` | `FlameAltarEvent::base_entity_id` |
| `AltarUsed::action` | `FlameAltarEvent::r#type` |
| `game_events::LootOpened` | `game_events::LootInteractionEvent` |
| `LootOpened::player` | `LootInteractionEvent::guest_id` |
| `LootOpened::container` | `LootInteractionEvent::host_id` |
| `LootOpened::take_all` | `LootInteractionEvent::loot_all` |
| `game_events::EquipmentChanged` | `game_events::HandleEquipmentSlotChangedEvent` |
| `EquipmentChanged::player` | `HandleEquipmentSlotChangedEvent::player_entity_id` |
| `EquipmentChanged::slot` | `HandleEquipmentSlotChangedEvent::equipment_slot` |
| `game_events::Revived` | `game_events::RevivedPlayerEvent` |
| `Revived::player` | `RevivedPlayerEvent::target_entity_id` |
| `Revived::by` | `RevivedPlayerEvent::source_entity_id` |
| `Block::player` | `Block::owner_id` |
- The other types, fields and constants the game names take the game's
  reflected names; the ones enshwick computes say so in their docs (#525):

| Old | New |
| --- | --- |
| `damage::DamageType` | `damage::DamageTypeNew` |
| `DamageType::Cut, Pierce, Blunt` | `DamageTypeNew::PhysicalCut, PhysicalPierce, PhysicalBlunt` |
| `DamageType::Fire, Ice, Shroud, Lightning` | `DamageTypeNew::MagicalFire, MagicalIce, MagicalFog, MagicalLightning` |
| `buffs::BuffKind` | `buffs::BuffSlotType` |
| `BuffKind::Terrain` | `BuffSlotType::TerrainDebuff` |
| `buffs::BuffId` | `buffs::BuffTypeId` |
| `BuffType::id, kind, duration, saved, hidden` | `BuffType::buff_type_id, slot, default_life_time, is_saved, hide_in_ui` |
| `Buff::id, entity, kind, duration` | `Buff::type_id, buff_entity_id, slot, life_time` |
| `knowledge::KnowledgeKind` | `knowledge::KnowledgeType` |
| `Known::kind, expected` | `Known::r#type, expected_flags` |
| `weather::FrontKind` | `weather::WeatherPhenomenonEffect` |
| `FrontKind::Clouds` | `WeatherPhenomenonEffect::None` |
| `affixes::Rarity` | `affixes::ItemRarity` |
| `affixes::Categories` | `affixes::ItemCategoryMask` |
| `Categories::INSTRUMENTS, FISHING_RODS` | `ItemCategoryMask::INSTRUMENT, FISHING_ROD` |
| `inventory::Category` | `inventory::InventoryCategory` |
| `Category::UNKNOWN` | `InventoryCategory::INVALID` |
| `ItemInfo::max_stack` | `ItemInfo::max_stack_size` |
| `content::types::ITEM_REGISTRY, RECIPE_REGISTRY, WORKSHOP_REGISTRY` | `content::types::ITEM_REGISTRY_RESOURCE, RECIPE_REGISTRY_RESOURCE, WORKSHOP_REGISTRY_RESOURCE` |
| `content::types::TEMPLATE, TEMPLATE_COLLECTION` | `content::types::TEMPLATE_RESOURCE, TEMPLATE_COLLECTION_RESOURCE` |
| `content::types::UI_IMAGE, UI_BUNDLE` | `content::types::UI_TEXTURE_RESOURCE, FB_UI_BUNDLE` |
| `content::items::Stat` | `content::items::ImpactUiValue` |
| `Stat::label, format` | `ImpactUiValue::loca_id, value_format` |
| `content::items::Meter` | `content::items::FuelSetup` |
| `Meter::max` | `FuelSetup::max_fuel_charges` |
| `changes::Field::MaxStack, Time` | `changes::Field::MaxStackSize, CraftingDuration` |
| `terrain::Material` | `terrain::TerrainTerraformingConfig` |
| `Material::id, item, health, flattens` | `TerrainTerraformingConfig::terrain_material, terrain_item, health_points, can_be_flattened` |
| `loot::Offer` | `loot::InteractionOfferRuntime` |
| `Offer::guest, accepted_at, disabled` | `InteractionOfferRuntime::guest_id, last_accept_time, is_disabled` |
| `Container::empty` | `Container::is_empty` |
| `effects::NpcOrder` | `effects::NpcCommandType` |
| `NpcOrder::GoTo` | `NpcCommandType::GoToPosition` |
| `TimeOfDay::day_length, night_length` | `TimeOfDay::day_time_duration, night_time_duration` |


### Removed

- `control_altar_zones`, `ZoneControl`, `ZoneSize` and `SetSizesError`
  (#499).
- `progress::AIRBORNE_ARCHITECT`, `OpResult::TOO_LARGE`,
  `OpResult::NO_ROOM`, `ExperienceSource::from_raw` and
  `AltarAction::from_raw` (#494).
- The game settings enums' `from_value` and `value` (#506).

## [1.0.0-alpha.4] - 2026-10-26

### Added

- `rates`: a mod declares a `Rate` with `Load::rate` and sets its factor;
  the loader scales experience by group, workshop speed, resource drops,
  comfort, skill points per level and loot stacks by the product of every
  mod's factor (#445).
- `Load::event_types()` lists every game event type the running game build
  has (#461).
- `RecipeView::kind()` and `RecipeKind` tell a building block, a roof,
  overgrowth or terrain from other recipes by the output item's data (#469).
- `i18n::MOD_NAME` and `i18n::MOD_DESCRIPTION`: a mod's texts give its name
  and description in the player's language on the Mods screen, in join
  notes, the console and the status summary (#470).
- A dedicated server runs lines typed into its console window or appended to
  `mod-data/enshwick/commands.txt`, as `enshwick ...` tools or as
  `<mod id> <action>` (#440, #452). The tools include `enshwick mods`,
  `world`, `entity`, `placed`, `reload`, `kick`, `ban` and `unban` (#455).
- A DLL in `mods/` without the Enshwick interface stays loaded as a native
  mod and is listed on the Mods screen and in `status.log` (#415).
- `logs/enshwick/report.md`, one file a player attaches to an issue (#418).

### Changed

- `enshwick` and `enshwick-gamedata` require the exact version of the
  crates they are released with, so an older `enshwick` never resolves a
  newer `enshwick-macros` (#484).

- A mod id without an author part is refused whatever ABI minor the mod was
  built for. Rebuild a mod built for ABI minor 69 or older with an
  `<author>.<name>` id (#436).
- Game events, content layouts and content types resolve through engine ids
  the loader maps to the running build; `content::types` are
  `ResourceType` handles (#460, #461).
- `rename_setting` and `remove_setting` take dotted keys and move a setting
  into or out of a table (#464).

### Removed

- `game_events::EVENT_TYPE_NAMES` and `Kind::TYPE_NAME` (#461).

## [1.0.0-alpha.3] - 2026-10-26

### Added

- Mod ids have an author part, `<author>.<name>`, each part of `[a-z0-9-]`,
  at most 64 characters; `abi::is_valid_id` checks one, and
  `cargo enshwick new` takes the new form (#378).
- `was: [...]` in `manifest!` for a renamed mod: it replaces its old ids,
  answers requirements and extension points named by them, and on its first
  start takes over the old id's settings file, texts and `mod-data` folder.
  `replaces: [...]` and `abi::is_valid_former_id` accept ids of the old form
  (#369, #378, #424).
- `Migration::take_setting` for a mod that folds another one in and takes its
  settings (#369).
- `points: [Type, ...]` in `manifest!`: a mod lists the extension points it
  defines, and their schemas go into its DLL. `cargo enshwick point` writes
  another mod's point definition from that DLL (#394).
- `#[config(group)]`: a setting that is a `Config` of its own becomes a table
  of the settings file, keyed `group.field` on the Mods screen and the
  console (#399).
- A list of up to 16 numbers with a range shows a slider per element on the
  Mods screen and takes `key[i]` on the console; `#[config(first = n)]` and
  the text `<key>.element` label the elements (#403, #425).
- `DamageCause::NAMED` lists every damage cause the API names (#400).
- `Texts::languages()` and `Texts::untranslated()` for a test that a mod's
  texts have no gaps (#397).
- The stub loads every DLL in `mods/chain/` before the loader, so another
  mod's own proxy DLL runs beside Enshwick (#414).

### Changed

- Game memory is read only as types valid for any bytes, through
  `enshwick::raw::read_at` and `read_ptr_at`. `abi::Headed` is an
  `unsafe trait` that requires `Zeroable` (#380).
- `loot::near` moved to `world::near` (#401).
- The fake host's builder is split by domain; `Installed::check_settings`
  returns `SettingsRefused` (#368).
- A number outside its range in a settings file is held to the range with a
  warning instead of refusing the whole file (#425).
- Of two files that declare one mod id, the highest version runs (#365).

### Removed

- `points::safes`, `points::survivors_per_altar`,
  `points::flame_altar_count` and `points::area_loot`: the API declares no
  mod's points. A mod that extended one writes its definition with
  `cargo enshwick point` (#394).
- The `enshwick-reflect` crate; its reader is `enshwick_gamedata::reflect`
  (#421).
- The probe and research mods moved to
  [devtools-mods](https://github.com/enshwick/devtools-mods) (#363).

## [1.0.0-alpha.2] - 2026-10-26

### Added

- A mod's actions run from the server's console as
  `<mod id> <action> <values>`, through `ActionCall::CONSOLE` (#336).

### Removed

- Chat commands: `commands`, `Reply`, `CommandContext`, `Load::add_command`,
  `Action::verbs` and `Actions::set_language`. Players and administrators
  use actions on the Mods screen, and servers the console (#336).

## [1.0.0-alpha.1] - 2026-10-26

The first version on crates.io, as `enshwick`, with `enshwick-macros`,
`enshwick-gamedata` and `cargo-enshwick` (#347).
