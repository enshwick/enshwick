### Added

- `Effects::notify` raises a notifier at a place or on an entity, which every client in range plays, a client without mods too; at most `MAX_NOTIFIES_PER_STEP` (32) per mod in one server step. `Load::on_notify` hears each notifier and sound the server places, and `game_events::NotifyEvent` every notify event. The book's sounds chapter shows both, in Rust and Luau.
