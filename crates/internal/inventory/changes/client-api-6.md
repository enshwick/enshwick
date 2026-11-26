### Changed

- The slot reads walk an inventory's links through one shared walk, which
  the loader's readers call too; it reads past an empty link instead of
  stopping at it (#1287).
