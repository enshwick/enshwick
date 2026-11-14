### Added

- `input`: `Input::local_status()` (Luau: `enshwick.input.local_status()`) observes cached local sampler availability and sample age without requesting reads. Requires a matching ABI 11.31 loader; the state is not world or session identity.

- ABI 11.31 appends a client-only cached sampler diagnostic getter. The loader and fake host expose availability and age without new game reads or input commands.
