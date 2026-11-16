### Added

- `Load::add_terrain_material` and `terrain::NewTerrainMaterial` add a
  terrain material of a mod's own: a copy of one of the game's, its look
  included, with its own drop item, hardness, health and flattening. Mods'
  materials take the 11 terrain ids the game leaves free, 117 to 127, in
  load order, and the twelfth is refused with its reason, as are a
  server-only mod, a GUID the game names and a hardness past `Unbreakable`;
  Luau
  `enshwick.terrain.add_terrain_material()` (#1131).
- ABI 12.2: `HostApi::terrain_material_add` with `TerrainMaterialAdd`
  (#1131).
- Fake host: added terrain materials take the free ids in order, are refused
  past them and for a material the game lacks, and `Terrain::materials`
  lists them (#1131).
- Loader: each material's palette entry, terraforming rules and effects
  entry are served on both sides, and the look of the material it copies on
  clients. `mods/enshwick/clones.toml` keeps every material with its
  fallback, and in a world loaded without the mod its ground loads as that
  fallback instead of air (#1131).
