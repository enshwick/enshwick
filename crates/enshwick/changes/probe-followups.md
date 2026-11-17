### Added

- `FakeHost::mod_file` puts a file, such as a developer tool's
  `settings.toml`, in the mod's own folder. Each install makes a new folder,
  `mods/<folder>` in the temp folder, so `Load::export_dir` finds it as the
  loader's would; it is removed when the install drops, so tests on threads
  of one process never read each other's files. `Installed::mod_dir` names
  the folder the mod is given.
