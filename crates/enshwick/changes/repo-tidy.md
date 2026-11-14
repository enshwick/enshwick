### Fixed

- The example mod `you.trail` gives its quest torch once per player: it keeps a mark in its world data, so a repeated `QuestCompleted` gives no second torch, and a request that gave nothing tries again at the next completion.
- The book's quest reward handler, in Rust and in Luau, keeps the same mark per player.
