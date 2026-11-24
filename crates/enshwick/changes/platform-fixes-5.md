### Added

- `input`: `CommandAction::AnswerDialog` answers the loader's dialog that shows, a mod's or the loader's own, with a response one of its buttons gives, observed once the dialog closes with it, behind `unstable`.
- `input`: `ClientSnapshot::dialog_buttons` and `dialog_mod` name the buttons of the dialog that shows and its mod, behind `unstable`.
- `ui`: `Response::ALL` and `Response::of_buttons`.
- `abi`: `ClientCommand` gains `answer` and the kind `ANSWER_DIALOG`; `ClientSnapshot` gains `dialog_mod`, `dialog_buttons` and the group `DIALOG`; ABI 12.25.

### Fixed

- `input`: a waypoint or ping whose metres reach 2^31 on an axis, past the game's fixed-point range, is refused on a joined client too.
- `Load::remove_quest` keeps a quest that another mod's step in a vanilla quest, lore page entry or item set asks for, not only another mod's own quest.
