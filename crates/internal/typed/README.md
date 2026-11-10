# enshwick-typed

Internal part of [Enshwick](https://github.com/enshwick/enshwick); depend on
[`enshwick`](https://crates.io/crates/enshwick), which re-exports every item
here as `enshwick::typed` with its `typed` feature.

Every component, event and resource the game reflects, as typed Rust items
generated from the game's own reflection by `cargo enshwick generate`.

Licensed under [MIT](https://github.com/enshwick/enshwick/blob/main/LICENSE).
