# Code patches

A code patch replaces a few bytes of game code: a constant in an instruction, a
jump, a loop bound. Reach for a [hook](hooks.md) first; patch only what a hook
cannot reach.

## Asking for a patch

A patch is the smallest change a mod can make, and the bluntest: the bytes
stay changed for every caller, and for the rest of the process.

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // `cmp eax, 8; jb` becomes `cmp eax, 16; jb`.
    let site = load.find_code("83 F8 08 72 ??")?;
    // SAFETY: an 8-bit immediate becomes another; the code stays valid.
    unsafe { load.patch_code(site + 2, &[0x08], &[0x10]) }
}
```

`load.patch_code(address, expected, replacement)` asks for the bytes at
`address` to read `expected` and become `replacement`. Both have the same
length. Find the address with a [code pattern](finding-the-game.md#code-patterns)
or a pinned symbol, never a fixed number. The call is `unsafe`: the loader
checks the bytes it replaces, never what the new ones do.

## Checks

When the mod asks, during `init`, `patch_code` returns `Off` when:

- the lengths differ or are zero;
- the bytes are not all inside the game's code section;
- another patch, of any mod or of the loader, already covers any of them;
- `init` is over.

At arming, after every mod is initialised, the loader checks each patch again
and skips it, turning its mod off, when:

- the bytes no longer read `expected`;
- they overlap the bytes a hook replaces: a hooked function's first
  instructions or a [mid-function hook's](mid-function-hooks.md) jump;
- one of the mod's hooks could not be armed.

The loader writes the patches that pass in the same batch as every hook's
jump, with the game's other threads suspended, and logs each one: written,
skipped with the reason, or left out because its mod is off.

## All or nothing per mod

A mod's patches go in together. When one of them is skipped, none of the mod's
patches is written, since a change made of several patches may be unsafe in
part. A mod that returns `Off` from `init` after asking for patches gets none
of them.

[the loader's base limit](https://github.com/enshwick/enshwick/blob/main/crates/internal/loader/src/bases.rs)
widens the game's base grouping from 16 to 32 zones with six patches
and three hooks, all found by patterns before any is registered. A
miss there logs a warning and leaves the game at 16 altar zones.

## Patches stay

A patch is written once and never undone. When its mod is turned off later,
after a fault, its hooks are unlinked but its patches stay for the process.
Write patches whose effect is safe without the rest of the mod.

## Patterns

A patch site is found by a code pattern, as in
[Code patterns](finding-the-game.md#code-patterns). Wildcard what changes
between builds, such as displacements and offsets, and include the bytes you
patch in the pattern: `expected` must match them at arming anyway.
