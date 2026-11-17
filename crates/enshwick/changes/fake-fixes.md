### Fixed

- `FakeHost`'s rebuild of a served resource now runs as the loader's: the
  patches of every mod that built the bytes of the game's first build run
  again, a mod turned off since included, over the base kept for it, then
  the keys whose builds read it; a rebuild of a key the game never built
  runs no patch.
- `FakeHost` refuses a rebuild as the loader does: from a mod that is off,
  of a key the mod does not serve, of a key whose base was not kept, and of
  a build of another length.
- The docs of `content::rebuild` and the host's `rebuild_resource` say a
  rebuild runs the patches of a mod turned off since the first build.
