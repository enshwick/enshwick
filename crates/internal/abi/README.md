# enshwick-abi

Internal part of [Enshwick](https://github.com/enshwick/enshwick); depend on
[`enshwick`](https://crates.io/crates/enshwick), which re-exports every item
here under its own paths.

The binary interface between the Enshwick loader and a mod: the `repr(C)`
types that cross it, its version, and the reads and writes of game memory.

Licensed under [MIT](https://github.com/enshwick/enshwick/blob/main/LICENSE).
