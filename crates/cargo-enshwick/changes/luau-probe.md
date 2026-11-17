### Fixed

- `enshwick.typed.d.luau` no longer declares again the typed game types
  `enshwick.d.luau` declares, such as `typed_FactionType`.
- `cargo enshwick types` keeps clear of the typed game types the API's
  definitions declare, by the names Luau sees.
