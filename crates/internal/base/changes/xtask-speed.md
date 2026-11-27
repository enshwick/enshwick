### Fixed

- In a build with the fake host's features, the scheduler counts its delays
  and periods on the fake's clock only once a fake host is installed, and on
  the wall clock before, so the loader's tests see time pass even when
  another crate of their build turns those features on.
