# Code style

> English Rust, as fast as the machine allows, safe by construction where it can
> be and careful where it cannot, with comments only where the code cannot carry
> the meaning.

The baseline is how the Rust community writes Rust: the
[Rust API Guidelines](https://rust-lang.github.io/api-guidelines/), the
[rustdoc book](https://doc.rust-lang.org/rustdoc/how-to-write-documentation.html),
the [Rust Style Guide](https://doc.rust-lang.org/style-guide/) and clippy. These
rules follow it and are stricter: where the community recommends, this
repository requires, and where it leaves a choice, the faster form wins. Across
code, tests, tooling and process, where practices differ the strictest one
applies, and a check that can be automated is automated rather than left to
review.

These rules bind all code in the workspace. Where a tool can check a rule,
`[workspace.lints]` in the root `Cargo.toml`, `rustfmt.toml`, `deny.toml` and
`_typos.toml` enforce it. Settings for the build profiles and the lint set are
recorded as [decisions](../decisions/).

Code that contradicts these rules is corrected in a change of its own, or keeps
its reason beside it (see Exceptions).

## Language

Everything committed to the repository is written in English: code, identifiers,
comments, commit messages, documentation, configuration. Player-facing strings
are data and may be in any language.

## Code and comments

Performance comes first, and readability stays non-negotiable beside it: the
fastest form is written so it reads well, never traded for one that reads better
and runs slower. When a construct needs a comment to be understood, the first
move is rewriting it until it does not.

Comments exist for what code cannot express: a constraint that surprises the
reader, an invariant, an engine fact, the reason behind a decision. Descriptions
of what the code plainly does, of why a change is correct, or of where it came
from belong nowhere in the repository. The default amount of comments is zero.

## Public API

The public API is the exception to the zero default, since other authors build
on it: every public item, every public module and every crate root carries a
rustdoc comment. They are short and to the point, never a wall of text:

- The first sentence is what an IDE shows on hover. It says why the item exists
  and when to reach for it; what it does is the job of the signature. Anything a
  reader learns from autocompletion is never written in words.
- The standard sections are present wherever they apply, each a line or two:
  `# Examples` on every public function, type and macro, with a short example
  that compiles as a doctest; `# Errors` on every function returning `Result`;
  `# Panics` on every function that can panic; `# Safety` on every `unsafe fn`
  and `unsafe trait`. Related items are linked with intra-doc links.
- `missing_docs`, `clippy::missing_errors_doc`, `clippy::missing_panics_doc`,
  `clippy::missing_safety_doc` and `rustdoc::broken_intra_doc_links` are denied,
  so a missing section fails the build.

Examples use `?`, never `unwrap`. Types follow the checklist of the Rust API
Guidelines: common traits derived where they apply, `Debug` on every public
type, private fields, sealed traits for traits mods must not implement, and
`#[non_exhaustive]` on public enums and option structs from the day they appear.
ABI types are the exception: they keep public `repr(C)` fields and grow only by
appending. `missing_debug_implementations`, `unreachable_pub` and every stable
`rustdoc::` lint are denied as well.

Internals stay private to their crate, and the public surface is re-exported
from the crate root.

## Performance

Speed is the first requirement. The loader and the API run inside the game's
frame, so every cycle they take is a cycle the game loses:

- Hot paths, every detour and per-frame pass, do not allocate, lock, format or
  log. State they read is atomic, thread-local or swapped whole; work that can
  wait goes to a background thread.
- The common case of a detour is the vanilla answer, and it is the fast path: a
  check that decides "not ours" comes first, costs a few instructions, and the
  rest sits behind `#[cold]` or in a separate function.
- Data is laid out for the cache: contiguous arrays over pointer-chasing
  structures, small fixed-size types, bounded collections on the stack.
- A crate that makes the code faster is taken: lock-free state, fast hashers,
  stack collections, SIMD, better allocators where they show up in a profile.
  Each passes cargo-deny like any dependency.
- Speed is measured, with a benchmark in the crate or a trace from the game. A
  change on a hot path names the numbers before and after.

## Unsafe

Hooking a game is unsafe by nature, so unsafe code is kept small and named:

- A crate that needs no unsafe code starts with `#![forbid(unsafe_code)]`.
- `unsafe_op_in_unsafe_fn` is denied: each unsafe operation sits in its own
  `unsafe` block, however small.
- Every `unsafe` block carries a `// SAFETY:` comment naming the invariant it
  relies on, such as an engine fact with the game build it was found on, and so
  does every `unsafe impl`. `undocumented_unsafe_blocks`,
  `multiple_unsafe_ops_per_block` and `unnecessary_safety_comment` check it.
- Raw reads of game memory go through a few typed helpers, never through
  scattered pointer arithmetic; offsets are named constants beside the type they
  describe.
- Nothing unwinds across the FFI boundary. Every detour and every function the
  loader calls in a mod catches panics and turns them into a disabled feature,
  never into a crash.

## Errors and logging

Errors are values: library code returns `Result` with an error type built with
`thiserror` that implements `Error + Send + Sync + 'static`, named `Error`
inside its module or `VerbObjectError`, with lowercase messages and no final
period. `anyhow` and `Box<dyn Error>` stay out of hot paths and the public API.
The loader decides at the top what a failure disables. `unwrap`, `expect`,
`panic!`, `todo!`, `exit` and printing are denied; a proven site carries
`#[expect]` with its reason. Logging is for the player's and the developer's log
file, once per event worth knowing, never per frame.

## Exceptions

Where code stands against a convention on purpose, the reason sits beside it,
since the reason is the one thing the code cannot carry. `#[expect(...)]` with
its `reason = "..."` is the form, so a stale exception is reported.

## Naming

A contract owns the good name: a trait is a plain noun, with no `I` prefix and
no `Trait`, `Base` or `Impl` affix. A concrete type is a qualifier in front of
its contract that names what it runs on, so `MinHookEngine` implements
`HookEngine`. A role rides as a suffix, such as `Error`, `Builder` or `Guard`,
and the module states the role of its items.

Conversions say their cost: `as_` borrows for free, `to_` costs, `into_`
consumes. Getters have no `get_` prefix. Iterator methods are `iter`, `iter_mut`
and `into_iter`, and their types share the name. Constructors are `new`,
`with_*` and `from_*`. Acronyms are one word (`Rpc`, not `RPC`), engine names
included. Features name what they add, without `use-` or `with-`, and are
additive.

Beyond that, names follow the Rust API guidelines: `snake_case` functions,
modules and files, `CamelCase` types and traits, `SCREAMING_SNAKE_CASE`
constants. Names say what a thing is in the game's terms (`Altar`, `Survivor`,
`BuildZone`), not how it is stored, and engine items are named after the
engine's own names where reflection gives them.

## Modules

`foo.rs` sits beside `foo/`, never `foo/mod.rs`. The public surface is
re-exported from the crate root; there is no prelude until a decision record
introduces one. Macro support items live in a `#[doc(hidden)] __private` module.

## Formatting

Tools settle the shape rather than taste: rustfmt writes the code, markdownlint
checks the structure of documents, typos reads the spelling. A review that
spends time on whitespace means one of them is missing from the loop; the
commands are in [repository.md](repository.md#checks).

## Documents

Documentation appears on explicit request, stays short, plain and direct, and
prefers prose wherever a sentence does the job;
[documentation.md](documentation.md) governs the rest.
