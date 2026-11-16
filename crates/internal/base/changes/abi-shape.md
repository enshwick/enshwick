### Changed

- `state::ModLocal` methods take `&'static self`, and its value type is `Send + Sync + 'static`.

### Fixed

- Two data packs no longer share one `ModLocal` value, so each pack's `World`, `Effects`, map, input, interaction targets and buff types act under its own handle.
