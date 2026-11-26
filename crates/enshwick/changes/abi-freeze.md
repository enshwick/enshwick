### Added

- `abi::ALPHA_MAJORS`, the ABI majors 5 to 12 of the alphas: a loader asks
  to rebuild a mod built for one of them instead of asking for a newer
  Enshwick (#1282).

### Changed

- **Breaking:** the ABI is stable at 1.0: `abi::ABI_MAJOR` is 1 and
  `abi::ABI_MINOR` is 0, so a mod built for ABI 1 loads on every loader of
  ABI 1. Every headed struct starts at its whole size, and the loader
  refuses one shorter than that. Rebuild every mod (#1282).
- **Breaking:** `abi::HostApi` places the entries added since ABI 12.0 with
  the other entries of their domain, so their slots move; the C header
  follows (#1282).
