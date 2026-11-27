### Fixed

- Chat mutes and mod key bindings go back to what their file holds when
  the loader gives up on a change kept for a held file, so the game no
  longer keeps a change the Mods screen calls lost (#1306).
- An equip of an item the player moved out of an equipment slot by hand
  puts it back even when no mod read the slots at the time (#1306).
- A world call refused for a malformed argument reaches the mod as
  `WorldError::BadArgument` (#1306).
