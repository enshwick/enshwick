### Added

- `Input::local_status()` reports the local input sampler's cached flags
  and sample age without starting reads; Luau
  `enshwick.input.local_status()` (#1118).
- `Input::client_snapshot()` copies the client's state for any thread: the
  session, the world, the open screen and the one it returns to, the
  screen's host, the in-game menu's tab, the crafting menu's highlighted
  recipe, the local player and its held controls; Luau
  `enshwick.input.client_snapshot()` (#1118).
- `Input::client_command()`, `cancel_command()` and `command_status()` open
  or close the game's screens, choose the menu's tab, turn the camera once,
  hold a direction for at most 250 ms and highlight a recipe in the open
  crafting menu, each with an id and an outcome the client's own state
  confirms; the player's own input cancels a look or move; Luau
  `enshwick.input.client_command()` (#1118).
- `mods::loader_off()` lists each loader part and feature that is off, with
  its reason and time; Luau `enshwick.mods.loader_off()` (#1118).
- `ItemInfo::placed_entity_template` names the template a player places
  from an item (#1118).
- ABI 12.1: `HostApi::local_input_status`, `client_snapshot`, `loader_off`,
  `client_command` and `client_command_status`, with `LocalInputInfo`,
  `ClientSnapshot`, `ClientCommand`, `ClientCommandStatus` and `LoaderOff`;
  `ClientCommand::in_bounds` checks a command's own bounds (#1118).
- Fake host: the client snapshot, the sampler status and client commands
  answer as the loader does, with bounds refused and statuses the test sets
  (#1118).
- Loader: one detour on the client's input builder writes a look or a move
  after the game's own pass, and one on the recipe directory column reads
  and highlights the crafting menu's recipe; each costs one atomic load
  until a mod hands it a command (#1118).

### Fixed

- `host_version` and the Mods screen's own row state a development build's
  version, as the log already did (#1118).
