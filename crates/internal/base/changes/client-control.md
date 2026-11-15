### Added

- `mods::loader_off()` and its Luau mirror list each loader part and
  feature that is off, with its reason and time (#1118).
- A test refuses an older or truncated host table before an appended slot
  is read (#1118).

### Fixed

- `host_version` states a development build's version (#1118).
