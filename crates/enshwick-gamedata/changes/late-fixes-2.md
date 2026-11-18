### Added

- The route `net.session_state` finds the online provider's state with a
  session, which its join passes to its state transition, on every kept
  client build.

### Fixed

- The route `net.provider_transition` finds the provider's state transition
  through its leave, which names no state, so it no longer holds only while
  the session's state is 2.
