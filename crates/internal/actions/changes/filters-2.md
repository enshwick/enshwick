### Changed

- `Placement::account` is `None` only in the prediction of a client that
  joined a server, and on the server for a character made in the step under
  way; a client that hosts the game gives its own prediction its player's
  account.
