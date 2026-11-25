### Fixed

- `World::can_build` and `World::persists` refuse a corner at 2^31 metres
  or more on an axis, as well as one not finite; `Weather::at` gives `None`
  for such a position.
