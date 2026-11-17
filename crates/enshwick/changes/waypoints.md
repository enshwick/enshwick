### Added

- `abi`: `Event::WAYPOINT_CHANGED` with `WaypointData`, and
  `HostApi::waypoint`, which reads a character's waypoint now, ABI 12.16
  (#1187).

### Changed

- Loader: one watcher reads the waypoints every 100 ms after the step,
  while a mod hears `Event::WAYPOINT_CHANGED` or a client holds a map. It
  raises the event and runs the maps of carried windows on each change.
  A build that does not reflect the waypoint lists "waypoints" as off with
  its reason, in place of "map waypoints" (#1187).
- Fake host: raises `Event::WAYPOINT_CHANGED` after a step and answers
  `HostApi::waypoint` from its components, as the loader does (#1187).
- Fake host: `set_waypoint` runs a map only for a waypoint inside the
  map's frame that moved, remembered while no map is held, as the loader
  does (#1187).
- Loader: the watcher stops once every mod that hears the event is off,
  and a panic in a map's run turns off only the maps, listed as "map
  runs" (#1187).
