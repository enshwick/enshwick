### Added

- With the `typed` feature, a client mod reads the keys the player bound to
  the game's own controls with `Input::binding`, rebinds one with
  `Input::rebind` and gives it back with `Input::reset_binding`; the
  loader writes a rebind again each time the game rebuilds its bindings,
  and a rebind is its mod's own, ending when that mod gives it back or
  turns off.
  `input::Binding`, `input::BoundKey` and `input::BindingError` go with
  them, and Luau has `enshwick.input.binding`, `rebind` and
  `reset_binding`. Unstable.
- ABI 12.20: `HostApi::game_binding`, `HostApi::game_rebind` and
  `abi::GameBinding`.
- The fake host names a game action with its keys through
  `Installed::bind_game_action` and reports a mod's rebind through
  `Installed::game_rebind`.

### Changed

- A mod's key bindings on the Mods screen say when the game also uses a
  control's key, in place of a note that the game's keys are not checked.
