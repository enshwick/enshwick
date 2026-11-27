### Changed

- Grants, revokes and sets of an id the game does not know, a value that
  does not fit its kind or a bit it does not expect return
  `WorldError::BadArgs` instead of `WorldError::Refused` (#1306).
