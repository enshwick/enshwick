### Changed

- A client that hosts the game gives its own build prediction its player's
  account, so a build filter that judges by account answers the host's own
  cursor as the server's world does. A client that joined a server still gives
  `None`. The fake host gives a hosting client's own prediction its
  `host_key`.
- `on_damage` registers on a build that lacks the hit's layout; then only
  `Impact` is off.

### Fixed

- A damage filter site that a game build lacks turns off only the causes it
  serves: hits, falls, the shroud, drowning, freezing and admin kills each
  arm on their own. The off line in the log, the status summary and the Mods
  screen names the causes the filters no longer get. Before, one missing site
  turned off every cause.
