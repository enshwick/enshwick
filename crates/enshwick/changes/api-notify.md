### Added

- `Effects::notify` (unstable) raises a notifier at a place or on an entity, which every client in range plays; at most `MAX_NOTIFIES_PER_STEP` (32) per mod in one server step, entity 0 and a position that is not finite refused. `Load::on_notify` hears every notify event the server raises but its visual effects, and `game_events::NotifyEvent` every notify event. The book's sounds chapter shows both, in Rust and Luau.
