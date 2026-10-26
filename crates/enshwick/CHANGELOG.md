# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change.

## [Unreleased]

### Added

- `Rate::RecipeOutput(kind)` multiplies what the recipes of each
  `RecipeKind` make, after every mod's own recipe patch;
  `RecipeEdit::scale_output_by` scales by a fraction and
  `RecipeKind::ALL` lists the kinds.

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
