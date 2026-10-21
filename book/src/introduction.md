# Introduction

Enshwick Loader is a mod loader and modding API for
[Enshrouded](https://enshrouded.com). The loader sits next to the game or the
dedicated server as `powrprof.dll`, and every mod is a DLL in the `mods/`
folder beside it. This book is for people who write those mods.

A mod is a Rust `cdylib` built against one crate,
[`enshwick-loader-api`](https://github.com/themaiby/enshwick/tree/main/crates/enshwick-loader-api),
with one tool,
[`cargo enshwick`](https://github.com/themaiby/enshwick/tree/main/crates/cargo-enshwick).
The crates under
[`crates/internal`](https://github.com/themaiby/enshwick/tree/main/crates/internal)
are parts of the loader; a mod never depends on them. A mod names itself in
a manifest, reads its settings, reacts to events, adds items and recipes,
answers chat commands, and where the API has no call, links hooks into game
functions. The loader keeps the
game running when a mod fails: it turns that mod off, and the game runs
vanilla where the mod acted.

Start with [Getting started](getting-started.md), then read
[Concepts](concepts.md) once. The other chapters stand on their own; take the
one you need. The API's rustdoc is the reference for every type and function
(`cargo doc -p enshwick-loader-api --features test --open` builds it); this
book says how they fit together, and its links to the API point at the source the rustdoc is built
from.

The mods in
[`mods/`](https://github.com/themaiby/enshwick/tree/main/mods) are worked
examples, and the chapters point at them where a mod does what the chapter
describes.

## Building the book

The book is an [mdBook](https://rust-lang.github.io/mdBook/). From the
repository root:

```bash
cargo install --locked mdbook
mdbook serve book
```

Its Rust examples compile as doctests of `enshwick-loader-api`, so
`cargo test --doc --workspace` checks them. `bun run lint:md` and `typos` check
the text.
