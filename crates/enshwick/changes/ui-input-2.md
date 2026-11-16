### Added

- `Frame::back` (Rust) and `frame:back()` (Luau) say when the player pressed
  Back (Escape, or B on a gamepad) over a mod's window. The press no longer
  opens the game's pause menu; the mod decides what Back does. The fake
  loader presses it with `Input::Back`.

### Fixed

- A mod settings slider shows the number it is dragged to while the drag
  lasts, as the game's own sliders do, instead of the stored value.
- Under a loader or mod window in a world, the character takes no input:
  besides the camera and movement, interacting, attacking, jumping, dodging
  and the hotbar keys wait until the window closes, as under the game's own
  screens. The cursor and this hold turn on and off together.
- Escape closes the loader's interface demo window without opening the
  pause menu.
