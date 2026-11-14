### Changed

- Loader: every entry of the host table checks its arguments before it acts,
  so a call with a null out pointer is refused with `BAD_ARGS` before
  anything happens, where some entries spawned, dropped or ran an extension
  point first (#1083).
- Loader: a mod that is off gets `FAULTED` from every entry it calls, also
  when it passes a malformed argument (#1083).
- Loader: a refusal the loader logs, such as a service published twice, is
  logged as the calling mod's warning with the reason, the same way for
  every entry (#1083).
- Loader: a lookup of one player reads that player alone instead of
  building the list of every player (#1083).

### Fixed

- Loader: a panic in the loader's code of an entry turns that one entry off,
  lists it for the status summary and the Mods screen, and later calls to it
  answer what the panic answered (`REFUSED`) once the caller is checked;
  before, the panic was only logged (#1083).
- Loader: a panic inside a mod's callback that an entry ran turns that mod
  off and blames it, instead of leaving the crash blame on the mod's marker
  (#1083).
- Fake host: an inventory request is refused as the loader refuses it, for
  more operations than a request holds, an invalid operation, or a client
  that hosts no game, so a mod's test fails where the game would (#1083).
