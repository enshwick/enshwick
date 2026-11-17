### Changed

- An `items.toml` change reads `max_stack` and `slots` only with the
  `unstable` feature; without it, the pack's reader names them as unknown
  fields. The loader builds the reader with the feature, so data packs keep
  both. Breaking.
