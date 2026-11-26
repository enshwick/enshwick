### Changed

- **Breaking:** a script error carries a kind, the Rust error's variant name
  or `BadArgs`, and Luau raises it as a table of `kind`, `message` and
  `call` (#1295).
