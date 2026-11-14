### Fixed

- Keep a carried window a mod shows a player whose client has not said hello yet, and show it when the hello arrives, so a window shown at join is no longer lost; a client without Enshwick holds at most eight until its player leaves (#1085).
- Answer an interface run whose text is longer than its argument takes with the argument's bound, as a typed line is answered (#1085).

### Changed

- Draw a carried window's fields without building their values or option lists each frame (#1085).
