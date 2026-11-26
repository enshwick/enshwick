### Changed

- **Breaking:** `Frame::play` returns a `UiError`, with `UiError::Missing` for
  a sound container that neither the game's archive nor a mod holds (#1295).
