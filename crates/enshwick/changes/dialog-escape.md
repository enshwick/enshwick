### Fixed

- Escape, or B on a gamepad, closes a loader dialog again without an
  answer, and the pause menu does not open. The press binds to the game's
  own popup window, so it no longer crashes the client. While a dialog
  shows over a loader or mod window, or the Mods screen, the dialog takes
  the press, and the window under it stays open.
- Back (Escape, B) reaches a loader or mod window, and the Mods screen,
  only while it was drawn in this or the last frame, so a press after the
  game's own popup closes, or under a Mods screen that turned off, no longer
  goes to a panel the game no longer draws.
- A mod window whose mod id and window id together run long no longer
  stops the client: the game takes window names of at most 64 bytes, so a
  longer name goes to the game as a short one made from its hash.
