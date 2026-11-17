### Fixed

- Mods screen: a mod without a settings button keeps a place as tall as
  the Details button the game draws, rather than a height measured at
  1280x720, so the Details button stays in the same place in every row at
  any resolution and interface scale, beside icons or text buttons (#1192).
- The loader's slot widget benchmark helper is built only for tests and
  the `test` feature, so the shipped loader no longer carries it (#1192).
