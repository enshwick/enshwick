### Fixed

- When a mod's rules for who may run its actions change, the server sends
  each client that mod's carried controls again, not those of every mod
  with actions, and asks no other mod which actions a player may run. A
  rules change that comes while a send is on its way goes at the next look
  (#1235).
- A mod whose commands turned off after a handler panicked in a typed line
  shows the feature `commands` off on the Mods screen and in the status
  summary, and `commands::list`, the console's `enshwick help` and the
  remote list leave its commands out (#1235).
- While the chat commands are off after a fault, a chat line that starts
  with `/` and a mod's command name or alias still leaves the chat, unrun,
  so the words typed for the mod reach no other player (#1235).
