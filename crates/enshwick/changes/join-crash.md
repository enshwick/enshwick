### Fixed

- Escape over a loader dialog does nothing; the dialog closes by its
  buttons. While a dialog shows over a loader or mod window, or the Mods
  screen, the press is dropped, so it neither closes the window under the
  dialog nor opens the pause menu.
- Back (Escape, B) reaches a loader or mod window, and the Mods screen,
  only while it was drawn in this or the last frame, so a press after the
  game's own popup closes, or under a Mods screen that turned off, no longer
  goes to a panel the game no longer draws.
