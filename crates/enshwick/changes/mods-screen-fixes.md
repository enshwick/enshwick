### Fixed

- Mods screen: the Details and settings buttons show the game's icons again;
  the small button's look is found among the button's looks, not across its
  other fields, and when the icons do not read the log names the check that
  failed.
- Mods screen: without icons, a gear that opens only key bindings, such as in
  the main menu, reads "Key bindings" instead of "Settings".
- Mods screen: a mod file refused at start (Windows does not load it, it was
  made for another Enshwick, its manifest does not read, or its folder holds a
  second mod) shows as off with its reason, and the main menu line, the chat
  line at world load and the status count it.
- Loader: a mod DLL whose manifest does not parse is unloaded.
