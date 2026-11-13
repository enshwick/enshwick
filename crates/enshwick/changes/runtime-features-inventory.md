### Fixed

- Loader: damage filters, player deaths, item drops, the survivor menu,
  inventory requests and stat maxima that a build lacks are listed as off
  on the Mods screen and in the status summary, not only in the log
  (#1069).
- Loader: a panic in a survivor feature turns off that feature and every
  hook of it, and the other survivor features keep running (#1069).

### Changed

- Loader: the damage filters arm at all nine sites or none, also when one
  site fails while the hooks arm, and a hit no filter or stat factor watches
  is no longer read; stat factors on hits need only the hit site (#1069).
- Loader: a build without the equipment system turns off only the item
  equips, not the item use filters and events (#1069).
- Loader: inventory requests already queued when inventory requests turn
  off are answered as unavailable at the next step, not timed out 30 s
  later (#1069).
