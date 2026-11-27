### Added

- `WorldError::BadArgs`, "an argument is malformed or unknown to this
  game build": what a world call returns for a malformed value, a value
  past its bounds or a name the build does not know (#1306).

### Changed

- A call the loader refuses for a malformed argument, and the safe layer's
  own checks of its arguments (positions not finite, empty boxes, account
  0), return `WorldError::BadArgs` instead of `WorldError::Refused`
  (#1306).
- `GameSettings::change` and `keep` return `WorldError::BadArgs` for a
  factor or length the game cannot take (#1306).
