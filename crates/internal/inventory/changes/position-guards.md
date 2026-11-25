### Fixed

- `Effects::npc_command`, `Effects::ping` and `Effects::notify` refuse a position not finite
  or at 2^31 metres or more on an axis with `WorldError::Refused`, before
  they raise the event; `spawn`, `drop_items` and `teleport` name the same
  refusal.
