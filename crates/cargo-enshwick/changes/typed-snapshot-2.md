### Changed

- `cargo enshwick generate --from` with a server build that has a kept
  table writes the typed API from the newest server build's snapshot, so
  retaking an older kept build's snapshot leaves the API as it is.
