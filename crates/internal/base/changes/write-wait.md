### Added

- `__internal::briefly`: runs one write with the calling thread's atomic file
  writes waiting about 30 ms per step for a held file, then gives the thread
  its own wait back (#1297).
- `__internal::held`: tells whether a file write failed because another handle
  holds the file (#1297).
