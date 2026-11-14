### Fixed

- Loader: a control is declared in one step, so two threads declaring the
  same control can no longer both be granted it (#1104).

### Changed

- Loader: every feature asks the mods' callbacks through one helper, which
  skips mods that are off and times each call; a refused registration, a
  setting write and the other converted calls answer the same statuses as
  before (#1104).
