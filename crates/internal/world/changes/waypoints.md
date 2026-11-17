### Changed

- `Map::waypoint` and `Map::snapshot` read the waypoint through the
  loader, and `Load::on_waypoint` hears the loader's
  `Event::WAYPOINT_CHANGED` in place of its own look every tenth of a
  second. On a build without waypoints, `on_waypoint` no longer fails: the
  loader lists the waypoints as off and the handler is never called (#1187).
