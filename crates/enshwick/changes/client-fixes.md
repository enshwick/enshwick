### Fixed

- `input`: `CommandAction::Host` plays a world and character whose saves sit in Steam's cloud storage, not only in the local one, where the game refused to create the game.
- `input`: `CommandAction::SetWindowText` keeps its text in a carried window's field, which the game's own text field had written back over at its next draw, and ends observed only once the drawn field holds it.
- A carried window's text field drawn again after its Run starts empty, instead of showing the last text and sending it again with the next Run.
- A server's carried windows close when the client leaves the world for the main menu, so none stays over the menu or keeps the Mods screen from opening.
