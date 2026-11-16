### Added

- `StatDef::in_group` and `stats::StatGroup`, with the `unstable` feature,
  show a stat kind's value for the local player as a row of a group of the
  Character screen, with its name and the description `<name>.description`
  (`stats::description_key`); the server sends each player's client the
  values of its own kinds with a group.
- Item tooltips on clients with Enshwick show a line per stat modifier of an
  item's own and of each affix it rolled, `+10% Luck`, before the lines of
  `Load::extend_item_tooltip`.
- `StatDef::hidden` shows a kind nowhere: no row and no tooltip line.
- `Load::define_stat` hands the loader the mod's `src/texts.toml` for a
  kind that shows, so its row and tooltip lines have its name without a
  call to `Load::texts`.
- The `item-ui` example defines Luck in the Basics tab and the weapon affix
  Lucky that crafted weapons roll.
- ABI 12.6: `StatSpec::group` and `StatSpec::flags` with `StatSpec::GROUP_*`
  and `StatSpec::HIDDEN`.
- Luau: `group` and `hidden` in a stat definition's table and
  `enshwick.stats.description_key`.
