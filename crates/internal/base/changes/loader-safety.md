### Added

- `__internal::served_content` and `__internal::inventory_queue`: the limits on
  the content mods serve in one process and on the inventory requests that
  wait, which the loader and the fake host share (#1294).
- `__internal::wait_briefly_on_this_thread`: makes the atomic file writes of the
  calling thread wait about 30 ms per step for a file another handle holds,
  for a thread that must not stall, or a second again (#1294).
- `__internal::while_busy`: tries a file step again for about a second while
  another handle holds the file, whatever the thread's wait (#1294).

### Changed

- `Host::serve_content` and the scripts' `serve_content` name the loader's
  limit on served content in their errors (#1294).

### Fixed

- A failed atomic write on a thread that waits briefly removes its temporary
  file once another program lets go of it, so none is left beside the file
  (#1294).
