# enshwick-fake

Internal part of [Enshwick](https://github.com/enshwick/enshwick); depend on
[`enshwick`](https://crates.io/crates/enshwick) with its `test` feature, which
re-exports this crate's `test` module as `enshwick::test`.

The fake loader for a mod's unit tests: it answers as the game and the loader
would, starts the mod through its exports and records what the mod asks for.

Licensed under [MIT](https://github.com/enshwick/enshwick/blob/main/LICENSE).
