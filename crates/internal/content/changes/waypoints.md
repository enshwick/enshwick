### Changed

- The growing content helpers size a served build from their own edits:
  one measuring pass of the same edits reserves the final length, so each
  build still takes one loan. `Room` and `Blob::over_with_room` are gone
  (#1187).
