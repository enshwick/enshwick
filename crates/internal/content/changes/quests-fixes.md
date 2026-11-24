### Fixed

- A quest removal follows each step's game query into the query database the game serves, mods' queries in, and refuses while one reads the quest or one of its steps; it reads the database only when another mod's step asks through a game query, and keeps the quest when the database does not read (#1262).
- The knowledge a game query reads joins its world and player twins, which share its id (#1262).
