### Fixed

- The fake host refuses a data pack or another mod an event, point, content
  id, stat or affix named in the mod's namespace, as the loader does (#1296).
- The fake host writes one element of a list setting by its key, such as
  `per_level[2]`, as the loader does (#1296).
- A dialog of a data pack or another mod names its owner in the fake host's
  client snapshot (#1296).
