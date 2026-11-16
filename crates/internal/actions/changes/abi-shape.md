### Added

- `Interaction::new`, `Interaction::with_item` and `Interaction::with_position` build an interaction for a test of a filter; with the `unstable` feature, `Interaction::with_moved` and `Move::new` add the move it carries.

### Changed

- `interactions::Interaction` is `#[non_exhaustive]`: code outside enshwick builds one with `Interaction::new`, so a field appended later leaves it as it is.
