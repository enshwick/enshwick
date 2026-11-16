### Fixed

- A message a server mod sends a client after its hello and before the
  client took the server's welcome, or while its link starts over, now
  arrives once the client takes messages, instead of being lost.
- A player whose slot clears or is taken by another player before the game
  removes them now raises `PlayerLeft` once, as who joined there.
- A game build without one building constant keeps the world loaded,
  shutdown, step and player left events; only the building events go off.
