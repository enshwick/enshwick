### Fixed

- Drop the notices a server sent for the client's next step when the client changes servers or its hosted game ends, so they no longer show in the next game (#1097).
- Forget the players' settings rates and the lobby a client's hosted game published into when that game ends (#1097).
- Answer a game event handler registered after the loader sealed them with `TOO_LATE`; one that raced the seal was dropped with `OK` (#1097).
