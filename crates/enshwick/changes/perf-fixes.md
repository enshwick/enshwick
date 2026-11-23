### Changed

- Luau runtime: a call into a Luau handler costs about 37% less. A damage
  filter that halves a fall went from about 1.26 µs to about 0.8 µs per hit,
  against about 34 ns in Rust (`cargo bench -p enshwick-luau --bench calls`,
  cases `handler/*`). An object's method is now looked up inside the virtual
  machine, and entering a handler reads the clock once less.
