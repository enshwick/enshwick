# Changelog

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and versions follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
Each entry names the pull request that made the change. What mod authors
see is in the changelog of `enshwick`.

## [Unreleased]

### Added

- The hidden `__internal::item_data` and `__internal::knows_item`, the
  loader's item lookup, moved here from `enshwick-inventory` so the
  domains above reach it without depending on inventory (#1056).
- The content domain's `Bound` implementations, beside their types (#1053).
- The crate: the content domain, cut out of `enshwick-core` (#1049).
- `__load`, the method traits of each module that core's `Load` forwards
  to, and `__fake`, what the fake loader and core's tests read here
  (#1049).
