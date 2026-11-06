# Writing a mod

The modding guide is now
[the Enshwick modding book](https://github.com/enshwick/enshwick/blob/main/book/src/SUMMARY.md),
in `book/` at the repository root. Start with:

- [Hello, world](https://github.com/enshwick/enshwick/blob/main/book/src/getting-started.md):
  the smallest mod as a data pack, a Luau script and a Rust mod;
- [Your first real mod](https://github.com/enshwick/enshwick/blob/main/book/src/first-mod.md):
  an item, its recipe, its texts and a setting;
- [Concepts](https://github.com/enshwick/enshwick/blob/main/book/src/concepts.md):
  the loader, the life of a mod, sides and hosting, the join check, threads;
- [FAQ and troubleshooting](https://github.com/enshwick/enshwick/blob/main/book/src/faq.md):
  the logs, and why a mod is off.

The rustdoc of `enshwick` is the reference for every type and function:
`cargo doc -p enshwick --open` builds it.
