# Mid-function hooks

Sometimes the value to change sits in a register halfway through a function,
and no function boundary is near it. `load.hook_at(address, handler)` runs
`handler` before the instruction at `address`, with the registers the game code
has there:

```rust
fn init(load: &mut enshwick_loader_api::Load<'_>) -> Result<(), enshwick_loader_api::Off> {
    // An instruction inside a function, pinned per build in the mod's rows.
    let site = load.symbol("my-mod.range_clamp")?;
    load.hook_at(site, |registers: &mut enshwick_loader_api::Registers| {
        registers.rax = registers.rax.min(64);
    })
}
```

The address comes from the mod's [build rows](finding-the-game.md#build-rows-in-the-manifest)
or from `load.find_code` plus an offset. The handler is a `Fn(&mut Registers)`
closure that is `Send + Sync + 'static`; it runs on whichever game thread
reaches the instruction.

## Registers

[`Registers`](https://github.com/themaiby/enshwick/blob/main/crates/enshwick-loader-api/src/abi.rs)
holds the 16 general-purpose registers and `rflags`.

- Writes to any field but `rsp` take effect when the game code goes on.
- `rsp` is the stack pointer at `address`, for reading the function's locals;
  writes to it are ignored.
- The loader saves and restores `xmm0` to `xmm5` around the handlers, so Rust
  code in the handler may use them freely. It does not save the upper halves of
  the `ymm` registers or the x87 state, and a handler cannot read or change any
  `xmm` value the game holds.

## Choosing an address

The loader replaces the 5 bytes from `address` with a jump to a stub and moves
the instructions that were there; after the handlers the moved instructions
run and the function goes on.

- `address` must start an instruction.
- Nothing may jump into those 5 bytes from elsewhere, such as a loop head or a
  branch target. The loader cannot check this. Pick an address inside a
  straight run of instructions.
- Control must not leave within those bytes: a return, an unconditional or
  indirect jump, or an interrupt among them makes the loader refuse the address.
- The bytes must not overlap another hook's jump. Function hooks keep their
  bytes first, so a mid-function hook within the first bytes of a hooked
  function is refused, and of two mid-function hooks whose bytes overlap, one
  is refused.

`hook_at` itself refuses, with `Off`:

- a second handler of the same mod on one address;
- a handler on an address another mod holds exclusively;
- a call after `init`;
- a loader older than ABI minor 11, which has no mid-function hooks.

The checks on the instructions run at arming, after every mod is initialised.
An address refused there gets no jump, and every mod with a handler on it is
turned off, with the reason in the log.

## Several mods on one address

Every mod's handler on one address runs, in load order, each seeing the writes
of the ones before. All of them share one stub and one jump.

## Panics

The handler runs on a copy of the registers. A panic in it is caught: the copy
is dropped, so the game's registers stay as they were, and the mod is turned
off. From then on its handlers are skipped; the game goes on unchanged.

## Testing

In [the test fake](testing.md), `mid_hooks()` lists the addresses a mod hooked
and `run_at(address, &mut registers)` runs its handlers there, as the game
reaching that instruction would, and returns how many ran.

Write the handler as [code in the game's frame](game-frame.md): it runs every
time the game passes the instruction.
