### Added

- `cargo enshwick generate --from` records the executable's Steam build in
  its snapshot, from `--steam-build`, else from its kept table, else from
  Steam's appmanifest beside it, and refuses a build without a kept table
  whose Steam build none of them names.

### Fixed

- `cargo enshwick generate` writes the typed API from the newest Steam
  build's snapshot, so retaking an older build's snapshot or copying one
  from a backup no longer rolls the API back. It names each snapshot that
  records no Steam build.
