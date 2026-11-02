# Mods in C

A mod is a DLL that exports five functions over the C ABI, so C, C++, Zig,
Odin or any language that can include a C header can write one. Here you build
the example mod
[`hello.c`](https://github.com/enshwick/enshwick/blob/main/examples/hello-c/hello.c).

## The header and the example

The header is
[`enshwick.h`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/include/enshwick.h),
written from the same Rust types the loader uses and checked against the ABI's
layout on every change. `hello.c` writes one line to the log when it starts.

## Write the mod

1. Copy `enshwick.h` next to your source file.
2. Fill a static `EnshwickManifest`: its `header` is the struct's size,
   `ENSHWICK_ABI_MAJOR` and `ENSHWICK_ABI_MINOR`; a mod needs at least `id`,
   `version` and `side`. `ENSHWICK_STR_INIT("text")` makes a string.
3. Define `enshwick_mod_abi`, `enshwick_mod_query`, `enshwick_mod_init` and
   `enshwick_mod_ready`; `enshwick_mod_shutdown` is optional. The header
   declares them with `ENSHWICK_EXPORT`, so the DLL exports them.
4. In `enshwick_mod_init`, call the loader through the `EnshwickHostApi` table
   it passes, such as `host->log(me, EnshwickLevel_INFO, line)`.

Names follow one pattern: Rust's `Status::OK` is `EnshwickStatus_OK`, the type
`HookSpec` is `EnshwickHookSpec`, and a constant such as `MAX_ZONES` is
`ENSHWICK_MAX_ZONES`. Every comment in the header is the Rust item's
documentation.

## Build it

On Windows with LLVM, or on Linux and macOS with clang and lld:

```sh
clang --target=x86_64-pc-windows-msvc -std=c11 -O2 -shared -fuse-ld=lld \
  hello.c -o hello_c.dll
```

`hello.c` uses no C runtime, so it also builds with
`-ffreestanding
-nostdlib -Wl,/NOENTRY` and needs no Windows SDK. A mod that
calls the C runtime links it as usual: with Visual Studio's `cl /LD`, `clang-cl`
or [cargo-xwin](https://github.com/rust-cross/cargo-xwin)'s SDK off Windows.

A DLL your mod imports loads only from your mod's folder, the game's folder
or the Windows system folder, never from the current directory or `PATH`.
Ship every DLL it needs beside it, or link the C runtime statically (`/MT`).

## Check it

Run `cargo enshwick manifest hello_c.dll` on Windows. It loads the DLL, asks it
for its manifest and prints the id, version and side. Then copy the DLL into the
game's `mods/` folder. The log shows `hello from C` under `you.hello-c` when the
loader starts it.

## Keep it current

A loader loads a mod built against an older header while
`ENSHWICK_ABI_MAJOR` is the same, so rebuild the mod against the `enshwick.h`
of a release whose `ENSHWICK_ABI_MAJOR` differs from the one you built with.
`enshwick_mod_abi` returns `ENSHWICK_ABI_MAJOR`. The
[ABI reference](abi-reference.md#versions) has the rules.
