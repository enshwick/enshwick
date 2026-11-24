### Fixed

- Loader: a game a client hosts applies its world's disabled recipes at every
  world load and applies an edit of the file within a second. The client
  took its own list, sent back to it through its own connection, as a joined
  server's list. It then kept the rules of that moment: none at the first
  world load of a game process, and never an edit while the world ran.
- Loader: players who join a game a client hosts get its disabled recipes.
