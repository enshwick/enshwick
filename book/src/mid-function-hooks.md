# Mid-function hooks

Sometimes the value to change sits in a register halfway through a function,
and no function boundary is near it. `load.hook_at(address, handler)` runs
`handler` before the instruction at `address`, with the registers the game code
has there:

```rust
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    // An instruction inside a function, pinned per build in the mod's rows.
    let site = load.symbol("my-mod.range_clamp")?;
    // SAFETY: the site starts an instruction that reads a range in `rax`, and
    // a smaller range is one the game takes.
    unsafe {
        load.hook_at(site, |registers: &mut enshwick::abi::Registers| {
            registers.rax = registers.rax.min(64);
        })
    }
}
```

## The address and the handler

The address comes from the mod's [build rows](finding-the-game.md#build-rows-in-the-manifest)
or from `load.find_code` plus an offset. The handler is a `Fn(&mut Registers)`
closure that is `Send + Sync + 'static`; it runs on whichever game thread
reaches the instruction. The call is `unsafe`: a register the handler writes
is a value the game code goes on with, which the loader cannot check.

## Registers

[`Registers`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs)
holds the 16 general-purpose registers and `rflags`.

- Writes to any field but `rsp` take effect when the game code goes on.
- `rsp` is the stack pointer at `address`, for reading the function's locals;
  writes to it are ignored.
- The loader keeps every vector register the game holds across the handlers:
  the `xmm` and `ymm` registers and, where the system turns AVX-512 on, the
  `zmm` and mask registers. Handler code may use any of them freely, including
  AVX code that ends with `vzeroupper` and AVX-512 code. It does not save the
  x87 state, and a handler cannot read or change any vector value the game
  holds.

## Cost

A stub costs a few nanoseconds per pass on top of the handlers. How it keeps
the vector registers is chosen per processor when the hooks are armed:

| Processor                  | What the stub does                                                                                                                                                                                            |
| -------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| no AVX                     | saves `xmm0`-`xmm5`; the calling convention keeps the rest                                                                                                                                                    |
| AMD with AVX, no AVX-512   | saves `ymm0`-`ymm15`                                                                                                                                                                                          |
| Intel with AVX, no AVX-512 | saves `ymm0`-`ymm15`; when their upper halves are zero, gives them back with `vzeroupper` so the SSE code after the hook keeps its speed                                                                      |
| AMD with AVX-512           | ORs the vector and mask registers together and saves, by what is not zero, `xmm0`-`xmm5`, `ymm0`-`ymm15` or `zmm0`-`zmm15`, then `zmm16`-`zmm31` and the masks on their own                                   |
| Intel with AVX-512         | reads the state in use with `xgetbv` (a 512-bit OR would lower the clock) and saves the same parts; reads it again after the handlers and zeroes `zmm16`-`zmm31` and the masks only where a handler used them |

Per call of a hooked 3-instruction function with an empty handler, release
build, Ryzen 9 9950X3D (`cargo test -p enshwick-loader --release --lib
measures_the_stub_cost -- --ignored --nocapture`, which also runs the paths of other
processors):

| Path                  | Upper halves clear | `ymm` in use | `zmm` in use |
| --------------------- | ------------------ | ------------ | ------------ |
| AMD, AVX-512 (chosen) | +7.3 ns            | +8.2 ns      | +8.7 ns      |
| Intel, AVX-512        | +21.2 ns           | +21.7 ns     | +22.1 ns     |
| AMD, AVX              | +5.9 ns            | +6.1 ns      | —            |
| Intel, AVX            | +6.3 ns            | +6.4 ns      | —            |
| no AVX                | +5.2 ns            | —            | —            |

## Choosing an address

The loader replaces the 5 bytes from `address` with a jump to a stub and moves
the instructions that were there; after the handlers the moved instructions
run and the function goes on.

- `address` must start an instruction.
- Nothing may jump into those 5 bytes after their first, such as to a loop
  head or a branch target. The loader refuses the address when a direct jump
  or call of the same function lands there, by the function's bounds in the
  unwind data of its module. It cannot see a jump through a table or a
  computed address, a jump from another function, or one in code without
  unwind data. Pick an address inside a straight run of instructions.
- Control must not leave within those bytes: a return, an unconditional or
  indirect jump, or an interrupt among them makes the loader refuse the address.
- The bytes must not overlap another hook's jump. Function hooks keep their
  bytes first, so a mid-function hook within the first bytes of a hooked
  function is refused, and of two mid-function hooks whose bytes overlap, one
  is refused.

`hook_at` itself refuses, with `Off`:

- a second handler of the same mod on one address;
- a handler on an address another mod holds exclusively;
- a call after `init`.

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
