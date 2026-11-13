### Fixed

- Loader: the interaction filters arm all their hooks or none; when one of
  them, such as the base stock, does not link on a build, every filter is
  off and the Mods screen says why, instead of the rest running without it
  (#1070).
- Loader: these parts are listed off on the Mods screen and in the status
  summary when the game build lacks what they need, where only the log
  named them before: chat lines, chat lines to one player, interaction
  filters, prop removal by transactions, craft filters, mod control keys
  and the controls that follow game actions (#1070).
- Loader: a panic while screening a craft request turns off only the craft
  filters (#1070).
