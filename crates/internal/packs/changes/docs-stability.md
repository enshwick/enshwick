### Changed

- An `items.toml` change reads `max_stack` and `slots` only with the
  `unstable` feature; without it, the pack's reader names them as unknown
  fields. Breaking.
