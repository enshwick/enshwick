### Added

- `damage::causes_off` lists the damage causes no filter of `Load::on_damage` gets on the running build, after init; Luau `enshwick.damage.causes_off()`. `DamageCause::to_abi` and `DamageCause::from_abi` convert a cause to and from its ABI number. Unstable (#1181).
- ABI 12.14: `HostApi::damage_causes_off` (#1181).
- Fake host: `FakeHost::without_damage_sites` plays a build without the damage sites of some causes, which `damage::causes_off` then lists (#1181).

### Changed

- `Load::add_terrain_material` and `terrain::NewTerrainMaterial` need the `unstable` feature, until the game shows a mod's material; Luau `enshwick.terrain.add_terrain_material()` is marked unstable (#1181).
- Item selections need the `unstable` feature, as the two fields they change do: `Load::change_items`, `content::changes::Items`, `ItemTraits`, `ItemsChange`, `Target::Items`, `Field::selectable`, `ValueError::NotSelectable`, `Installed::item_changes` and `Pack::selections`. A data pack's item selection read without the feature is refused as one that changes nothing (#1181).

### Fixed

- Fake host: a raw change of a stack size or container slots, which a mod built without `unstable` cannot name, is taken or refused as the loader does, fixed and live, instead of always refused (#1181).
- Fake host: a live change starts as "times 1" whatever its record says, as the loader starts it (#1181).
