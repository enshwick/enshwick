### Added

- `check [dir]` checks a mod in any form: a data pack or Luau mod as
  `pack check` reads it, then every `.luau` script type-checked with
  luau-lsp against the definitions in `types/`, and a Rust mod with clippy
  for Windows; each problem prints as
  `<file>:<line>:<column>: <severity>: <message>`, and an error exits with
  1 (#1017).
- `check` downloads the luau-lsp release it was built for once into the
  user's cache folder and checks its SHA-256 when none is on the path;
  `--offline` skips the download and says so (#1017).

### Changed

- `pack check` names a misspelt or unknown field of pack data with the
  fields its place takes and the one it is near (#1017).
- The READMEs `new --luau` and `new --data` write check with
  `cargo enshwick check` (#1017).
- Depends on the internal crates `enshwick-core` and `enshwick-abi`
  instead of `enshwick`; `new` still pins `enshwick` at the same version
  (#1043).
