### Added

- `releases::stated` gives the version a build states: the release's own,
  or for a development build of a commit the release with `+dev.<commit>`,
  which keeps the release's precedence and channel; `releases::version_line`
  names the ABI version beside it (#1114).
- Routes find the client's input builder (`input.client_build`), its stick
  and look fields, `toggleHudScreen` (`ui.toggle_hud_screen`), the in-game
  menu's tab index (`ui.menu_tab`), the recipe directory column
  (`ui.recipe_directory`) and the recipe browser's highlighted recipe and
  select slot on every kept client build, with checks on the functions'
  first instructions (#1114).

### Changed

- The compatibility reports count the 8 new routed entries (#1114).
