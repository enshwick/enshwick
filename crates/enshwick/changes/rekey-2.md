### Added

- ABI 12.19: the `rekey_resource` host function serves a key from itself
  as a re-key of another key; inside its patch, a served read of that key
  builds it without its own re-keys (#1236).

### Fixed

- `Load::rekey_item` and Luau `enshwick.content.rekey_item`: re-keys that
  form a ring of three or more items rotate, where each item took the data
  of the item two steps on. The re-keys of every mod apply as one
  permutation: `old` takes `new` with every mod's changes but its re-keys,
  so in a chain of A to B and B to C, A takes B's data (#1236).
