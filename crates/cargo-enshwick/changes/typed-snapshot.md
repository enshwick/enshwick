### Changed

- `cargo enshwick generate --from` writes the snapshot of every reflected
  type to `reflection/` of the game data folder, and `generate` covers it by
  the `relevant.toml` of the repository it writes into, whichever tree built
  the tool.
