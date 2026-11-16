### Changed

- `effects::Effects` is one pointer to what the mod resolved once, whatever features it resolves, so a copy stays cheap as features are appended. Its methods take `self` instead of `&self`, so code that names one as a function item takes an `Effects` by value.
