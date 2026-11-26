### Added

- `abi::ALPHA_MAJORS`, the ABI majors 5 to 12 of the alphas: a loader asks
  to rebuild a mod built for one of them instead of asking for a newer
  Enshwick, and no stable major takes one of them (#1282).

### Changed

- **Breaking:** the ABI is stable at 1.0: `abi::ABI_MAJOR` is 1 and
  `abi::ABI_MINOR` is 0, so a mod built for ABI 1 loads on every loader of
  ABI 1 at its minor or a newer one. Every headed struct starts at its whole
  size, and the loader reads no struct and fills no list whose entries are
  shorter than that. Rebuild every mod. The ABI 12 minors that other notes
  of this release name never reached a release: ABI 1.0 holds everything
  they added. A behaviour those notes give a mod of an older minor, such as
  a panel kept centered at its edge, is gone with them (#1282).
- **Breaking:** `abi::HostApi` places the entries added since ABI 12.0 with
  the other entries of their domain, so their slots move; the C header
  follows (#1282).
