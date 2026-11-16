### Added

- ABI 12.7: `HostApi::transaction_settled` and `abi::TransactionSettledFn`.
  The loader calls a mod back once, after the step that ends a
  transaction's run or once a bound passed.

### Changed

- `World::when_settled` waits through the loader. Its handler runs after
  the step that writes the transaction whole, a scheduler pass sooner
  than before. Opening the world no longer subscribes a tick for the mod.
  When a hosted game ends, the handler runs at once with the transaction
  failed or expired, never in the next game.
- A transaction whose undo data expired still lists the props it placed,
  which stand in the world, until the world ends.
- Fake host: `end_game` fails or expires the game's transactions as the
  loader does and runs every `World::when_settled` handler still waiting;
  waits run only after a step, never at a world's load.
