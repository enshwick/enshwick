### Added

- `__internal::served_content`: the limits on the content mods serve in one
  process, which the loader and the fake host share (#1294).
- `__internal::wait_briefly_on_this_thread`: makes the atomic file writes of the
  calling thread wait about 30 ms per step for a file another handle holds,
  for a thread that must not stall (#1294).

### Changed

- `Host::serve_content` and the scripts' `serve_content` name the loader's
  limit on served content in their errors (#1294).
