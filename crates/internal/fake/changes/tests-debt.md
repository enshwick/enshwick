### Fixed

- A data pack or another mod is refused an event, point, content id, stat or
  affix in the namespace of the mod under test, as the loader refuses it (#1296).
- `set_config` takes a list element key such as `per_level[2]` (#1296).
- A dialog of a data pack or another mod names its owner, `test.mod-<handle>`,
  in the client snapshot (#1296).
- A data change is read before the phase, so a malformed one after init is a
  bad request, and an affix's owner is checked before the phase, as on the
  loader (#1296).
