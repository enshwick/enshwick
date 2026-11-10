# Welcome

A Luau mod for Enshrouded, run by the [Enshwick](https://github.com/enshwick/enshwick) loader.

It greets each player who joins and counts their visits. An administrator
presses F8 to tell everyone how many players are online. The mod runs on the
server only, and the server sends this key to each player's Mods screen,
where it shows under the mod's Key bindings.

1. Edit `main.luau`. `types/enshwick.d.luau` declares the API for
   [luau-lsp](https://github.com/JohnnyMorganz/luau-lsp); point the editor's
   `luau-lsp.types.definitionFiles` setting at it as `@enshwick`.
2. Run `luau-lsp analyze --definitions:@enshwick=types/enshwick.d.luau main.luau` to
   check the types without the game.
3. Copy this folder into the game's `mods/` folder as `mods/you.welcome/`.
4. Run `cargo enshwick pack zip .` for the archive players install.
