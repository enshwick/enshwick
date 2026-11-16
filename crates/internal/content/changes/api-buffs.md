### Changed

- `content::icons::ready`, `Room`, `Blob::over_with_room` and
  `Blob::reading` are reachable from the other internal crates, so a buff
  type names its icon only on a client that shows served icons and the buff
  collection builds in place (#1153).
