### Added

- `CommandAction::Chat` sends a line to the world's chat as the local
  player's own, on any server the client plays on, through the game's own
  chat; observed once the game's interface queue takes it.
  `abi::ClientCommand` gains the kind `CHAT` and `MAX_CHAT`. ABI 12.29.
