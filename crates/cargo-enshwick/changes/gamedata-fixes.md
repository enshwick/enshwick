### Added

- `cargo enshwick generate --from` records the executable's Steam build in
  its snapshot, from `--steam-build`, else from Steam's appmanifest beside
  it, else from its kept table.

### Fixed

- `cargo enshwick generate` writes the typed API from the newest Steam
  build's snapshot, so retaking an older build's snapshot or copying one
  from a backup no longer rolls the API back. It names each snapshot that
  records no Steam build.
