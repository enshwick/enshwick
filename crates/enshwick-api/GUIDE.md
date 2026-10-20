# Writing a mod

The modding guide is now
[the Enshwick modding book](https://github.com/themaiby/enshwick/blob/main/book/src/SUMMARY.md),
in `book/` at the repository root. Start with:

- [Getting started](https://github.com/themaiby/enshwick/blob/main/book/src/getting-started.md):
  `cargo enshwick new`, a first mod, installing it into the game;
- [Concepts](https://github.com/themaiby/enshwick/blob/main/book/src/concepts.md):
  the loader, the life of a mod, sides and hosting, the join check, threads;
- [FAQ and troubleshooting](https://github.com/themaiby/enshwick/blob/main/book/src/faq.md):
  the logs, and why a mod is off.

The rustdoc of `enshwick-api` is the reference for every type and function:
`cargo doc -p enshwick-api --open` builds it.
