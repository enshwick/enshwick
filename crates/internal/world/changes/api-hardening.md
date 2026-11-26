### Added

- `world::PlayerRef` names one player by account key, character or
  `Player`, and the map, chat, notice, build-right and respawn calls take it
  (#1295).

### Changed

- **Breaking:** `World::entities`, `near` and `containers` return
  `Result<_, WorldError>`, and `set_label` and `altars::set_base_limit`
  return typed errors (#1295).
- **Breaking:** `on_join` and `on_rename` take a rule that answers anything
  `Into<Verdict>` (#1295).
- **Breaking:** `GameSettings::read` returns a `SettingsRead`,
  `PlayerPermissions::ALL` is `&'static [Self]`, `World::component` is
  unstable, `BuffLayout` leaves the stable surface and `NewBuff::saved` is
  gone (#1295).
- **Breaking:** in Luau, `map.uncover` takes a vector and the fog of war's
  methods stay on its own object (#1295).
