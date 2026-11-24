### Added

- `Catalogue::all` reads every reflected type of a build, with no roles.
- `Catalogue::covered` keeps the types a `Relevance` rule gives a role and
  the types they reach.
- `Relevance` reads a `relevant.toml` rule; `Relevance::committed` is this
  crate's own rule, and `RELEVANCE` names its path in the repository.
- `SNAPSHOTS` names the snapshot folder of the game data folder.

### Changed

- A build's snapshot keeps every reflected type in `reflection/` of the game
  data folder, so one snapshot serves every rule. Each tree's tests apply
  their own `relevant.toml` to it, and a snapshot written in one tree no
  longer changes the test results of another.
