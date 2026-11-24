### Changed

- `cargo enshwick install` and `cargo enshwick pack` build a mod's DLL as a cdylib alone, so the release profile's fat LTO reaches it; built together with the rlib for the tests, it linked without LTO.
