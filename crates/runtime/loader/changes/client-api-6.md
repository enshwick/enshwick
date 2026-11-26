### Changed

- The player's slot read, the worn equipment read and the prop state read
  walk an inventory's links through the API's one shared walk. The prop
  state read now stops at a setup's first empty link, as the other reads
  always did, instead of reading past it (#1287).
