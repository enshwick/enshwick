### Added

- `vetoes::Verdict`, the one answer of every filter, with `From<bool>` and
  `From<Option<bool>>` (#1295).

### Changed

- **Breaking:** `EngineFacts` is sealed, `ui::Response::ALL` and
  `schema_section::Kind::ALL` are `&'static [Self]`, and
  `services::CallServiceError` and `state::StoreFileError` name the
  errors; `services` is stable (#1295).
