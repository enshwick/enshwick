# Code style

> English Rust, kept clean and fast, safe by construction where it can be and
> careful where it cannot, with comments only where the code cannot carry the
> meaning.

These rules bind all code in the workspace. Where a tool can check a rule,
`[workspace.lints]` in the root `Cargo.toml`, `rustfmt.toml`, `deny.toml` and
`_typos.toml` enforce it. Settings for the build profiles and the lint set are
argued in [research](../research/) and recorded as [decisions](../decisions/).

Code that contradicts these rules is corrected in a change of its own, or keeps
its reason beside it (see Exceptions).

## Language

Code, identifiers, comments, commit messages, configuration and documents are
written in English. Player-facing strings are data and may be in any language.

## Code and comments

Readability and performance are both non-negotiable, and neither buys the other.
When a construct needs a comment to be understood, the first move is rewriting
it until it does not.

- Every public item, and every module, opens with a doc comment of one or two
  sentences saying what it is for and when to reach for it.
- Inside a body, a comment exists only for what code cannot express: a
  constraint that surprises the reader, an invariant, an engine fact, the reason
  behind a decision.

Descriptions of what the code plainly does, of why a change is correct, or of
where it came from belong nowhere in the repository.

## The API surface

The loader's API is what other authors build on, so every public item of the API
crate carries a doc comment whose first sentence is what an IDE shows on hover.
`# Safety` is mandatory on every `unsafe fn`, `# Panics` and `# Errors` where
they are part of the contract, and `# Examples` on the entry points mod authors
call directly. Internals stay private to their crate; the public surface is
re-exported from the crate root.

## Unsafe

Hooking a game is unsafe by nature, so unsafe code is kept small and named:

- `unsafe_op_in_unsafe_fn` is denied: each unsafe operation sits in its own
  `unsafe` block, however small.
- Every `unsafe` block carries a `// SAFETY:` comment naming the invariant it
  relies on, such as an engine fact with its source in the research notes.
- Raw reads of game memory go through a few typed helpers, never through
  scattered pointer arithmetic; offsets are named constants beside the type they
  describe.
- Nothing unwinds across the FFI boundary. Every detour and every function the
  host calls in a mod catches panics and turns them into a disabled feature,
  never into a crash.

## Performance

Code that runs inside the game's frame is held to a budget:

- Hot paths, every detour and per-frame pass, do not allocate, lock or log in
  the common case. State they read is atomic, thread-local or swapped whole;
  work that can wait goes to a background thread.
- The common case of a detour is the vanilla answer, and it is the fast path: a
  check that decides "not ours" comes first and is cheap.
- Claims about speed are measured, with a benchmark in the crate or a trace from
  the game, before code is made harder to read for them.

## Errors and logging

Errors are values: library code returns `Result` with a typed error, and the
loader decides at the top what a failure disables. `unwrap` and `expect` appear
only where the invariant is proven beside them. Logging is for the player's and
the developer's log file, once per event worth knowing, never per frame.

## Exceptions

Where code stands against a convention on purpose, the reason sits beside it,
since the reason is the one thing the code cannot carry. An `#[allow(...)]`
carries that reason in its `reason = "..."` field, and `#[expect(...)]` is
preferred so a stale exception is reported.

## Naming

Names follow the Rust API guidelines: `snake_case` functions, modules and files,
`CamelCase` types and traits, `SCREAMING_SNAKE_CASE` constants. Names say what a
thing is in the game's terms (`Altar`, `Survivor`, `BuildZone`), not how it is
stored. Engine addresses and offsets are named after the engine's own names
where reflection gives them.

## Attribution

Everything here reads as human work. Attribution to any tool, generated-with
links and co-author trailers stay out of commits, pull requests, code and
documentation; the hooks and CI refuse them.
