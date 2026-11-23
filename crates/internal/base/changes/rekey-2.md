### Added

- `resources::rekey` and `resources::rekey_value` serve a key from itself
  as a re-key of another key; the shared build routine builds a re-key's
  served read of that key without its re-keys (`Build::Unkeyed`), marks
  the re-key running on its thread and checks a re-key's arguments
  (`rekeys_another`), for the loader and the fake host alike (#1236).
