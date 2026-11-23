### Added

- Loader: after its `ready: <n> of <m> mods running` line, the loader's log
  has one warning per mod that is not running, with its id, version, the
  first line of its reason and the error code, so the loader's log alone
  names every mod that did not start.
