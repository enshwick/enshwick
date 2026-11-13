### Changed

- `Effects::drop_items` no longer holds the first drop's server frame:
  the loader reads the pickup as the server starts, before its world
  loads, and a drop before that read ends waits for the rest of it (#1064).
