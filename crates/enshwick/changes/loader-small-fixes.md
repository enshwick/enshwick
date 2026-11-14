### Changed

- A mod's texts need English and Ukrainian only; a language without a key
  shows the English text. The loader's own texts follow the same rule
  (#1103).
- Loader: a client's hosted game's end forgets the players seen joining and
  the queued building events with the other parts' state (#1103).

### Fixed

- Loader: a data pack that turned off keeps its file hash in the join
  refusal, so the player sees whether their files differ from the server's
  (#1103).
- Loader: a mod dialog that belongs to a world closes with it when a hosted
  game ends or the client changes servers, and its mod hears the closed
  answer; a server mod's dialog and one a client mod queued in a world belong
  to it (#1103).
- Loader: whether the host of the client's lobby runs Enshwick, and the
  lobby entry's check, end at the client's next join, so a vanilla server
  joined after a modded friend's game gets no hello, and entering the same
  lobby again checks the host's mods again (#1103).
- Loader: one move of a world's event streams reads at most 64 streams, the
  engine's ceiling, on the server's world too (#1103).
