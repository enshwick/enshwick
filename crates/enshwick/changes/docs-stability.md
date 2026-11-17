### Changed

- `content::changes::Field::MaxStackSize` and `content::changes::Field::Slots`
  compile only with the `unstable` feature, as the stability chapter says. A
  mod without the feature gets a build error where the loader refused the
  change at run time before, and `Pack::parse` in such a mod names a pack's
  `max_stack` and `slots` as unknown fields. Data packs the loader reads keep
  both. Breaking.
