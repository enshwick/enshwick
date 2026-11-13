### Fixed

- `derive(Script)` badges the impls it writes in the API's own crates with
  the `unstable` feature when they build with `--cfg enshwick_docs`; a mod's
  crate gets no badge (#1077).
