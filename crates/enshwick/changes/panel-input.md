### Fixed

- A screen keeps its keys, tab clicks and hover while a mod's panel shows beside it: the Backpack closes on Escape and switches tabs again, and so do the chest screens. Only a click on the panel's own widgets goes to the panel.
- A panel beside a screen of the in-game menu shows only on its own tab: switching from the Backpack to the Character tab no longer leaves the Backpack's panel over the Character screen. A panel on the in-game menu itself still shows on every tab of a menu opened as a whole, and a panel beside the Social tab draws above the menu.
- With the `unstable` feature, `Frame::open_screen` names the in-game menu's tab the player sees, not the tab the menu opened on, and `Load::on_screen_changed` fires on a tab change. A server mod's window steps aside for the Map tab as it does for the Map screen.
- The item-ui example draws its panel at the Backpack's left edge, under the backpack's slots, instead of over the game's item details.
