### Added

- `Window::map(WindowMap)`: a window from the server holds a map. Its
  markers (position, label and the values their run gives) show on the
  player's own Map screen while they hold the window, and the waypoint the
  player sets there runs the map's `on_marker` action with the values of a
  marker within 16 m, else its `on_point` action with the waypoint's x, y
  and z. The server checks each run as any run, and a player gets the window
  only when they may run both actions. Unstable; Luau `{ Map = { markers,
  on_marker, on_point } }` (#1143).
- Loader: the server keeps the map of each window a client holds from the
  text it carried and looks at those players' waypoints every 100 ms; the
  client puts the markers of the newest held window's map on the game's map
  as the loader's own, steps its windows aside while the Map screen is open
  and draws a line saying to open the map. A build that does not reflect the
  waypoint lists "map waypoints" as off with its reason (#1143).
- Fake host: `Installed::set_waypoint` runs what the newest map a player
  holds asks for at a waypoint, as the loader does, and only when the
  waypoint moved (#1143).
