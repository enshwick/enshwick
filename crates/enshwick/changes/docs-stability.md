### Changed

- `content::changes::Field::MaxStackSize` and `content::changes::Field::Slots`
  compile only with the `unstable` feature, as the stability chapter says, and
  a data pack's `max_stack` and `slots` read only with it. A mod without the
  feature gets a build error where the loader refused the change at run time
  before. Breaking.
