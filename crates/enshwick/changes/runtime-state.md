### Fixed

- Refuse a `content::rebuild` that a served resource patch asks for its own key, on the thread that builds it, with an error; it waited on its own build and hung the game (#1089).
- Forget a hosted world's item data, scene markers, map frame and fog of war copy when the hosted game ends, so the next world a client hosts starts from its own (#1089).
