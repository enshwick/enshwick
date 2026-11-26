### Changed

- **Breaking:** the headed structs and `HostApi` are `#[non_exhaustive]`;
  `headed!` and `Headed::blank()` build a headed struct outside the crate.
  The ABI version stays 1.0 and the layout file is rewritten in place (#1295).
- **Breaking:** `Entity`'s field is private and `Entity` has no `Default`;
  `Entity::from_raw` builds one and refuses 0 (#1295).
- **Breaking:** a script that passes 0 as an entity gets a `BadArgs` error
  (#1295).
