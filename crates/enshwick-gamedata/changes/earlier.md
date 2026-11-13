### Added

- Routes for the client's row frame (`ui.row`), its separator line
  (`ui.separator`, with where it reads its margin, height and offset) and
  the text field's look (`ui.text_field_look`), found on every kept client
  build; the compatibility reports list them (#1023).

### Changed

- The crate depends on `enshwick-base` and `enshwick-content` instead of
  `enshwick-core`, so it no longer builds every domain of the API (#1049).
- Depends on the internal crate `enshwick-core` instead of `enshwick`; its
  types are the same ones `enshwick` re-exports (#1043).
