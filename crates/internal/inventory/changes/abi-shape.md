### Changed

- `effects::Effects` is one pointer to what the mod resolved once, whatever features it resolves, so a copy stays cheap as features are appended.
