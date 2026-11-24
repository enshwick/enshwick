### Added

- `input`: `CommandAction::SetWaypoint`, `ClearWaypoint` and `Ping` set, move and clear the local player's waypoint and ping a position, behind `unstable`.
- `input`: `CommandAction::Host` hosts a world from the main menu by its place in the menu's lists, behind `unstable`.
- `input`: `CommandAction::OpenMods`, `OpenMod` and `OpenModSettings` open the Mods screen, a mod's page and its settings gear, and `ClientSnapshot::loader_screen` and `loader_screen_mod` name the page that shows, behind `unstable`.
- `input`: `CommandId::as_u64` and `name()` on `CancelReason`, `JoinFailure`, `JoinPhase` and `LoaderPage`.
- `abi`: `ClientCommand` gains `position`, `world`, `character` and `online` and the kinds `SET_WAYPOINT` to `OPEN_MOD_SETTINGS`; `ClientSnapshot` gains `loader_screen` and `loader_screen_mod`; ABI 12.23.

### Fixed

- `input`: a press whose own held action opens a screen, as an interaction opens a chest, ends observed, not cancelled as another screen.
