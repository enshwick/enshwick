### Fixed

- A control rebound, a mod setting changed or a chat mute set while another program holds its
  file no longer freezes the game for up to about 2 s: the write waits about
  30 ms, then a thread of its own writes the latest text once the file is let
  go, and the game's end writes what still waits (#1297).
- A setting change that waits for a held file shows in the Mods screen, the
  mod's reads and what a server sends its players (#1297).
