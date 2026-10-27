# The game's own types

For a struct the API does not wrap, such as a component a
[hooked system](ecs.md) hands you, `cargo enshwick types` writes the game's own
types as Rust or Luau from the game's reflection data. The names are the game's:
`keen::ecs::Health` becomes `game::keen::ecs::Health`, a field `dataStorage`
becomes `data_storage`, and an enum constant `MagicalFog` becomes `MAGICAL_FOG`.

## Write the types

Name the types you need, or a namespace ending in `::`. The types they use come
with them:

```sh
cargo enshwick types --from "$ENSHROUDED_DIR" keen::ecs::Health \
  --rust src/game.rs --luau game.d.luau
```

`--from` takes the game folder or one of its executables. Without names, every
runtime type comes out, over six thousand of them; the editor's `keen::ds::`
copies come only when named. Add `pub mod game;` to `src/lib.rs`.

## Read a struct from game memory

Every struct is `repr(C)` with each field at the offset the game reflects, and
reads through [`enshwick::raw`](finding-the-game.md). A `bool` is its byte and
an enum is a newtype over its integer, so any bytes the game holds read as a
value. Check at `init` that the running build still has the size the module
expects:

```rust
# mod game {
#     pub mod keen {
#         pub mod ecs {
#             /// `keen::ecs::Health`: 40 bytes, aligned to 4.
#             #[repr(C, align(4))]
#             #[derive(Clone, Copy, Debug)]
#             pub struct Health {
#                 _pad_0: [u8; 8],
#                 /// `dataStorage`, at 0x8.
#                 pub data_storage: [i32; 8],
#             }
#             // SAFETY: integers only, so all bytes zero are a valid value.
#             unsafe impl enshwick::raw::Zeroable for Health {}
#             // SAFETY: integers only, so any bytes are a valid value.
#             unsafe impl enshwick::raw::AnyBitPattern for Health {}
#         }
#     }
# }
use game::keen::ecs::Health;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    if load.type_size("keen::ecs::Health")? as usize != size_of::<Health>() {
        return Err(enshwick::Off::new("keen::ecs::Health changed size"));
    }
    Ok(())
}

/// Reads the health component at `component`.
///
/// # Safety
///
/// - `component` points at a live `keen::ecs::Health` no thread writes now.
unsafe fn health(component: *const u8) -> Health {
    // SAFETY: the caller guarantees a whole `Health` at `component`.
    unsafe { enshwick::raw::read_at(component, 0) }
}
```

A field Rust cannot place where the game has it, such as an eight-byte integer
at an offset of four, comes out as bytes; one the game does not describe stays
in padding, and the struct's documentation names it.

## Use them from Luau

`game.d.luau` declares each struct as an exported record type, each enum as a
union of its constants' names, and keeps the game's field names. Point luau-lsp
at it beside `enshwick.d.luau`. It holds types only, and no Luau call returns
game structs.

## Write them again after an update

Offsets and sizes change between game builds, and the file names the build it
came from. Run the command again on the new build and rebuild the mod. Until
then, the size check at `init` turns the mod off on a build where the type grew
or shrank; for a field that matters, compare `load.field_offset(type, field)`
with its offset too.
