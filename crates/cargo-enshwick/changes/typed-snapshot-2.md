### Changed

- `cargo enshwick generate --from` writes the typed API from the newest
  server build's snapshot, also after it snapshots an older build, so
  retaking an older build's snapshot leaves the API as it is.
