### Fixed

- `Load::remove_quest` logs and leaves out a removal while a step of another mod's quest asks for the quest or one of its steps, by its id or through a game query such as one of `Load::add_query`, following the queries it names in turn; before, only a step that named the quest's id itself held it (#1262).
- Quest events follow the knowledge both the world and the player twin of a game query read, not only one twin's (#1262).
