### Fixed

- The fake host refuses served content past the loader's limit of 3072 contents
  or 512 MiB, as the loader does (#1294).
- The fake host answers `BUSY` to `inventory_submit` while 4096 requests wait,
  as the loader does (#1294).
