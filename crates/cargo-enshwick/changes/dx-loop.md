### Added

- `cargo enshwick run` installs a mod crate, data pack or Luau mod into a
  dedicated server, starts it and prints the loader's and the mod's log lines
  until a world has loaded, then a line per mod, running or off with the
  loader's reason. Ctrl-C stops only the server it started, in order, and
  `--watch` rebuilds, reinstalls and restarts on a change in the mod's folder,
  and starts a server that exited again after the next change that builds.
  Without `--watch`, a crash makes the tool fail. It says when the server's
  `enshwick.toml` keeps fewer log lines than the run needs.
- `cargo enshwick logs [-f] [--mod <id>] [--level <level>]` prints the newest
  run's log lines, every source merged by time.
- `cargo enshwick pack zip` packages a mod crate as its release archive: the
  release DLL under `mods/<id>/`, its id, version and side checked against
  `Cargo.toml` on Windows.

### Changed

- `cargo enshwick pack zip` names the archive `<id>-<version>.zip`, writes the
  same bytes for the same files and prints the archive's SHA-256; the folder
  is the current one when not named.
- `cargo enshwick install` also installs a data pack or Luau mod folder, and
  removes the pack files an earlier install left that the pack no longer has,
  with the folders that leaves empty. It stops with the reason when those
  files do not read as a pack.
