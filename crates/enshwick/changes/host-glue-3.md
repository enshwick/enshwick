### Changed

- Loader: every domain returns errors of its own, and only the layer that
  answers mods turns them into statuses; every call answers the same status
  as before (#1108).
- Loader: the survivor, altar, interface, stats and actions callbacks are
  asked through the one helper that skips mods that are off and times each
  call (#1108).
