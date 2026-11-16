### Added

- `Grow::reserve` makes room for a build's final length, so a served
  output takes one loan (#1136).
- Fake host: `Installed::is_off` tells whether the fake turned the mod off,
  and `FakeHost::refuse_tick` refuses one periodic tick subscription, as
  the loader refuses a tick it cannot list (#1136).

### Changed

- A served resource's builder that panics turns off only that resource:
  the loader builds the key without it until the game restarts, calling it
  again only in a rebuild of what the game holds, and the log, the status summary, the join line in chat
  and the Mods screen name the resource off. The mod and its other
  resources stay on; a builder's `None` still turns the mod off (#1136).
- A periodic tick handler also runs once at each world's start, before
  the world load handlers, unless it is paused, so a scheduler task queued
  in a world load or join handler of a new hosted game belongs to that
  game (#1136).
- A Luau mod opens `input` only when one of its scripts names it: a pack
  that never names `input` starts no client input reads (#1136).
- The content helpers reserve a build's final length before they copy the
  base: listing an item in a 256 KiB item registry takes one loan instead
  of three (#1136).
- Fake host: a mod whose init or ready fails, or that faults, is off, and
  none of its callbacks runs after that, as in the loader (#1136).

### Fixed

- Fake host: a prop a transaction places has its position, and loses it
  when the transaction is undone (#1136).
