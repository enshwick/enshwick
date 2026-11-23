### Added

- `resources::rekey` and `resources::rekey_value` serve a key from itself
  as a re-key of another key; the shared build routine builds a re-key's
  served read of that key without its re-keys (`Build::Unkeyed`), for the
  loader and the fake host alike (#1236).
