### Changed

- `cargo enshwick new` on a release of Enshwick writes the requirement
  `enshwick = "1.0"`, which takes every later release of the major, instead
  of the exact `=1.0.0`. A mod keeps loading under every loader of its major.
