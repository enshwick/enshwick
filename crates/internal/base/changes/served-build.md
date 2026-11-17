### Changed

- The loader and the fake host build served resources through one routine
  in `__internal::builds`, which picks and runs the patches, turns a failure
  into its outcome and decides when the first build keeps its base (#1184).
