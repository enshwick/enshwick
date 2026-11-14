### Changed

- `typed::EventType::on_client` never runs its handler on a dedicated
  server, where it answers without an error (#1081).
