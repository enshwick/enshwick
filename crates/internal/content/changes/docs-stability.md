### Changed

- `Field::MaxStackSize` and `Field::Slots` compile only with the `unstable`
  feature, so the API reference marks them unstable. A mod without the
  feature no longer builds a change of them that the loader then refuses.
  Breaking.
