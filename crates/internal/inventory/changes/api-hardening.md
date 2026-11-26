### Changed

- **Breaking:** every `Reader` read, `Inventories::count` and `slots`, and
  `Buffs::has` and `types` return `Result<_, WorldError>` (#1295).
- **Breaking:** `container_rule` takes a rule that answers anything
  `Into<Verdict>` (#1295).
- **Breaking:** the errors are `RunBatchError`, `RunOpError` and
  `SetItemDataError` (#1295).
