# Introduction

Enshwick is a modding API platform for [Enshrouded](https://enshrouded.com).
This book is for people who write mods with it. The Enshwick loader comes with
the platform and runs every mod, so players install one thing.

## What a mod is

A mod takes one of three forms:

- a Rust mod: a `cdylib` built against one crate,
  [`enshwick`](https://github.com/enshwick/enshwick/tree/main/crates/enshwick),
  with a typed API;
- a [Luau mod](luau.md): a folder of scripts, with no compiler;
- a [data pack](content/packs.md): a folder of TOML files, with no code.

One tool,
[`cargo enshwick`](https://github.com/enshwick/enshwick/tree/main/crates/cargo-enshwick),
starts each of them. The crates under
[`crates/internal`](https://github.com/enshwick/enshwick/tree/main/crates/internal)
are parts of the loader, and a mod never depends on them.

A mod names itself in a manifest. It reads its settings, reacts to events, adds
items and recipes, and offers players actions in the game's interface. It can
carry its texts in every language the game offers ([Texts](content/texts.md)).
Its unit tests run without the game against a fake host ([Testing](testing.md)).
Where the API has no call, a Rust mod links hooks into game functions. When a
mod fails, the loader turns that mod off, and the game runs vanilla where the
mod acted.

## How to read this book

Start with [Getting started](getting-started.md), then read
[Concepts](concepts.md) once. Each other chapter covers one subject; read the
one you need. The API's rustdoc is the reference for every type and function
(`cargo doc -p enshwick --features test --open` builds it). This book says how
they fit together. Its links to the API point at the source the rustdoc is built
from.

The examples in
[`examples/`](https://github.com/enshwick/enshwick/tree/main/examples) and our
mods, one folder each in the [mods repository](https://github.com/enshwick/mods),
are full working mods.

## Building the book

The book is an [mdBook](https://rust-lang.github.io/mdBook/). From the
repository root:

```bash
cargo install --locked mdbook
mdbook serve book
```

Its Rust examples compile as doctests of `enshwick`, so
`cargo test --doc --workspace` checks them. `rumdl check .` and `typos` check
the text.
