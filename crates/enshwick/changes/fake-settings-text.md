### Added

- `FakeHost::mod_file` puts a file, such as a developer tool's
  `settings.toml`, in a folder of the install's own, written once the
  install takes its turn and removed when it drops, so tests on threads of
  one process never read each other's files. `Installed::mod_dir` names the
  folder the mod is given.
