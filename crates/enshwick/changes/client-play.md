### Added

- `unstable`: client commands press a game action for 1 to 2000 ms, as its key would: `CommandAction::Press { action, hold_ms }`, Luau kind `"press"` with `action` by its game name. The loader sets the action's bit in the local player's input after the game builds it, on every step of the hold, so jump, sprint, block, the quick-use slots, an interaction (`ContextualActionTap`) and an attack (`MainhandActionTap` with `MainhandActionRelease`) act as the player's keys do, hosted and on a joined client. Several presses hold at once beside a look or a move; a mod holds one press of each action and four at most, up to eight across all mods, and one that waits two seconds for a free one is cancelled with `TimedOut`. A press ends at its hold, on a key the player presses after it started, on another screen, a new session, the game's input block, a cancel or when its mod turns off, and the bit is the game's own again in the same step.
- Append `ClientCommand::PRESS` and `ClientCommand::MAX_PRESS_MS` in ABI 12.21.
- `test`: `FakeHost` takes presses as the loader does: a `GameAction` name the host's reflected `PlayerInputType` names, one press of each action and four at most, and unavailable for an action the host lacks, whatever screen it expects.

### Fixed

- A client look or move that waits for another mod's to end is cancelled with `TimedOut` after two seconds, as a command that waits for its effect is; one the builder takes late holds its whole hold, timed from when it was taken.
