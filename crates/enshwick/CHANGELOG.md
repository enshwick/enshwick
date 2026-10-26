# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Added

- `World::can_build` tells whether the server lets a player edit a box:
  `CanEditWorld`, and `CanEditBase` inside a building zone, as the game's
  terrain tools decide, plus two tests from the game's building cursor, the
  world's building area and strict no-build zones; a refusal is a
  `BuildRefusal` with the game's `CursorFlag` names (#547).

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
