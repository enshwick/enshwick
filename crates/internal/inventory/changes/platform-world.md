### Changed

- `Effects::drop_items` no longer holds the first drop's server frame:
  the loader reads the pickup as the world loads, and a drop in the first
  tenth of a second after it waits for the rest of that read (#1064).
