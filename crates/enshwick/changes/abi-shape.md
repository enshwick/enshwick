### Added

- `Interaction::new`, `Interaction::with_item` and `Interaction::with_position` build an interaction for a test of a filter; with the `unstable` feature, `Interaction::with_moved` and `Move::new` add the move it carries.

### Changed

- `interactions::Interaction` is `#[non_exhaustive]`: code outside enshwick builds one with `Interaction::new`, so a field appended later leaves it as it is.
- `effects::Effects` is one pointer to what the mod resolved once, whatever features it resolves, so a copy stays cheap as features are appended. Its methods take `self` instead of `&self`, so code that names one as a function item takes an `Effects` by value.
- `state::ModLocal` methods take `&'static self`, and its value type is `Send + Sync + 'static`.

### Fixed

- Two data packs no longer share one `ModLocal` value, so each pack's `World`, `Effects`, map, input, interaction targets and buff types act under its own handle.
