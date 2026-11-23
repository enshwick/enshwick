### Added

- ABI 12.19: the `rekey_resource` host function serves a key from itself
  as a re-key of another key of its type; inside its patch, a served read
  of that key builds it without its own re-keys. A re-key to itself or to
  another type is a bad argument (#1236).

### Changed

- `Load::rekey_item` and Luau `enshwick.content.rekey_item`: the re-keys of
  every mod apply at once, each from the data its new item has before any
  re-key. In a chain of A to B and B to C, A now takes B's data, where it
  took C's (#1236).

### Fixed

- `Load::rekey_item` and Luau `enshwick.content.rekey_item`: re-keys that
  form a ring of three or more items rotate, where each item took the data
  of the item two steps on. A re-key joins the ring once its mod is built
  against ABI 12.19 (#1236).
