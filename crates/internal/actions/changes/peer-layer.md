### Added

- `Actions::on_refused`, `Refused` and `RefusedReason`, and the script's
  `on_refused` (#1094).

### Changed

- Every reader of a typed line and of the interface's values lives in one
  module, and a value is checked once at the boundary (#1094).
- `Registry::allowed` runs once per run (#1094).

### Fixed

- Arguments from the interface that do not decode answer with the usage, not
  the text of a missing right (#1094).
- `Action::admins` is the permission to kick and ban, so it holds beside an
  `allow` (#1094).
