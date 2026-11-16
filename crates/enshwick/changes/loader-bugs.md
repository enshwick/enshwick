### Added

- `FakeHost::vanilla_templates` gives the fake game's template collection
  names, so a test names vanilla templates as a console does (#1134).

### Changed

- A console names an action's item or template argument in any case when
  one item or template has that name; an exact name still wins. A name
  several share is refused with their GUIDs, at most eight and then how
  many more (#1134).

### Fixed

- A client that hosts a game tells the hosted server's world from its own
  by the server's thread from its start. Before, a build without the
  post-step link, or a panic in the step's work, left client game events,
  notices and the freeing of handed lists stopped for the whole hosted game,
  with no reason shown (#1134).
- A client without the server's state link lists its notices off with the
  reason, and its own game events too when a mod subscribed to them, since
  it cannot tell a hosted world from its own (#1134).
- A second Enter after a text field in the Mods screen or a mod's window
  saved its text no longer presses the window's Back button. The arrows
  still move the selection, and Back takes Enter once reached (#1134).
