# enshwick-core

Internal part of [Enshwick](https://github.com/enshwick/enshwick); depend on
[`enshwick`](https://crates.io/crates/enshwick), which re-exports every module
here at its own paths.

`Load`, the handle a mod's `init` gets, whose methods forward into the
domains' crates, and the script binding's namespace lists.

Licensed under [MIT](https://github.com/enshwick/enshwick/blob/main/LICENSE).
