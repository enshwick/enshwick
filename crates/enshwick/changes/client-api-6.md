### Fixed

- `FakeHost` refuses a client command while one of its kind waits only for
  the same mod, as the loader does, so another mod's waypoint, crafting or
  item request, or command of the same kind no longer refuses a mod's own;
  a mod reads and cancels only its own commands (#1287).
