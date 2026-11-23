### Changed

- `cargo enshwick logs` and `run` read the log files and the loader's start
  lines through the definition the loader writes them by.
- `cargo enshwick run` reads each log level key of `enshwick.toml` on its
  own, so one bad key no longer hides the others; one bad entry in
  `log_levels` drops that whole table, as the loader does.
