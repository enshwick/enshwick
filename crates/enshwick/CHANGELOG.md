# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Added

- A Luau mod reads and changes inventories through `enshwick.inventory`:
  `slots`, `count`, `item_info`, `give`, `take`, `destroy_slot`,
  `move_slot` and `move_all`, the done function getting the `Outcome` as a
  table with `Done` and `OpError` by name. `Inventories::slots` and
  `Inventories::count` read an inventory on the server's game thread
  without a `Reader` (#581).
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
  and answer have no padding, which `Shape::PADDING_FREE` reports.
- `Load::on_described_mod_event` and `Load::define_described_event`
  (`services::DescribedEvent`, `DescribedEmitter`) hear and send a mod
  event described at run time, its data as bytes in the definer's layout;
  `manifest!(events: [..])` puts the schema of each listed `#[event]` into
  the manifest, and `ModEvent::SCHEMA` and `DESCRIBED` carry it.
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
  the loader checks the layout hashes as for a typed link.
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
  scripts (a mask as its bit names, a value as its name).
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
  `Schema::template` writes the file.
- A pack's `pack.toml` declares settings in `[[setting]]` tables
  (`PackSetting`, `PackManifest::settings_schema`), checked as it is read.
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

- The loader's profile names the survivors feature `survivors`; it showed a
  mod id.
- The loader and the fake leave a mod event subscription idle when it
  reads more bytes than the definer sends.
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
