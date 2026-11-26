### Changed

- The player's slot read, the worn equipment read and the prop state read
  walk an inventory's links through the API's one shared walk. The player's
  slot read and the worn equipment read now read past an empty link, as the
  game's own setup check does, instead of stopping at it (#1287).
