### Fixed

- Refuse a `content::rebuild` that a served resource patch asks for its own key, on the thread that builds it, with an error; it waited on its own build and hung the game (#1089).
- Forget a hosted world's item data, scene markers, map frame, fog of war copy and deaths not yet raised when the hosted game ends, so the next world a client hosts starts from its own (#1089).
- Write the item data a mod sets in its `ShutdownStarted` handler of a hosted game; it was lost (#1089).
