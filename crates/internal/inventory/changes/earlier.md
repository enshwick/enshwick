### Added

- The inventory domain's `Bound` implementations, beside their types
  (#1053).
- The crate: the inventory domain, cut out of `enshwick-core` (#1052).
- `__load`, the method traits of each module that core's `Load` forwards
  to, and `__internal`, what core calls here and a mod must not (#1052).

### Removed

- **Breaking:** the hidden `__internal::knows_item`, now in
  `enshwick-content` (#1056).
