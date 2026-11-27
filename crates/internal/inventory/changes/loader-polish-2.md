### Changed

- Effects, drops, notifies and pickup messages return
  `WorldError::BadArgument` for a malformed argument instead of
  `WorldError::Refused` (#1306).
