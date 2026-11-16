### Changed

- `World::when_settled` waits through the loader's
  `HostApi::transaction_settled`. Its handler runs after the step that
  writes the transaction whole, and opening the world no longer subscribes
  a tick for the mod. When the world ends, it runs with the transaction
  failed or expired, never in the next game.
- `Settled::props` still lists the props of a transaction whose undo data
  expired, so `Settled::standing` gives its first prop.
