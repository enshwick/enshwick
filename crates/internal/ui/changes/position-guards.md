### Fixed

- `Ui::set_markers` refuses a marker whose position is not finite or at
  2^31 metres or more on an axis, and the mod's markers stay as they were.
- `sound::play`, `sound::play_for` and a sound's `set_position` refuse
  such a position too, and a near audience centred there.
