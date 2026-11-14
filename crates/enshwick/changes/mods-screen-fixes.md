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
  line at world load and the status count it. The reason says whether to
  update the mod or Enshwick, and a manifest mistake asks for a fixed version
  instead of a new download; a second mod file beside a mod that runs does not
  count as off.
- Loader: a DLL whose export table does not read, such as a mod's own helper,
  no longer takes its folder's mod slot or shows as a mod.
- Mods screen: without icons, the gear reads "Settings" only where its page
  shows a setting; in another's world a mod with only secret settings shows
  "Key bindings".
- Loader: a mod DLL whose manifest does not parse is unloaded.
