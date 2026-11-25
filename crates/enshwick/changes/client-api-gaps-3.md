### Added

- `input`: `ClientSnapshot::prompt_target` and `prompt_verb` name the entity the interaction prompt that shows targets and its verb, behind `unstable`.
- `input`: `ClientSnapshot::selected_character` names the character the main menu has selected, by its place in the menu's list as `CommandAction::Host` takes it, behind `unstable`.
- `abi`: `ClientSnapshot` gains `prompt_target`, `prompt_verb` and `character`, with the groups `PROMPT` and `CHARACTER` and `NO_CHARACTER`; ABI 12.26.

### Changed

- The Mods screen names the game action that shares a mod control's key by the game's own label of it, in the player's language.
