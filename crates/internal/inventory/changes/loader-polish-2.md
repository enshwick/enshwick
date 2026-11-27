### Changed

- Effects, drops, notifies and pickup messages return
  `WorldError::BadArgs` for a malformed argument instead of
  `WorldError::Refused`, and a drop of an item the game does not know
  does too (#1306).
