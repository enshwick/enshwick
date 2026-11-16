### Added

- `resources::Output`, the lent bytes a served builder writes into, and
  `Registry::encode_into`; the value builders encode straight into them
  (#1124).

### Changed

- A served builder takes `(base, out)` and returns `Option<()>` (#1124).
