### Changed

- `Effects::drop_items` no longer holds the first drop's server frame;
  it answers `WorldError::NotLoaded` for about a tenth of a second after
  the world loads, while the loader reads the pickup (#1064).
