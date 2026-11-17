### Fixed

- A panel added with `Load::extend_screen` shows beside the Backpack,
  Character, Map, Journal, crafting and Skill Tree screens: its window now
  draws just above the in-game menu that draws those screens. Beside every
  screen it stays below that screen's popups, such as the split stack menu,
  and below the tooltips and the item being dragged.
- The `item-ui` example registers its server half (the Luck and Calm each
  joining player gets, and the Lucky affix on crafted weapons) on every
  side, so it works in single player and in a game a client hosts.
