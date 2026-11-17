### Changed

- Item selections need the `unstable` feature: `Items`, `ItemTraits`,
  `ItemsChange`, `Target::Items`, `Field::selectable` and
  `ValueError::NotSelectable`. Breaking (#1181).
- Every field's value rules live in one table, which the fake host also
  reads for a raw change of the fields a build without `unstable` cannot
  name (#1181).
