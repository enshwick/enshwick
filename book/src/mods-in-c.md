<!--
type: how-to
description: A mod is a DLL that exports four functions over the C ABI, so C, C++ or any language that can call a C ABI can write one. Build the example mods in C and C++.
-->

# Mods in C and C++

A mod is a DLL that exports four functions over the C ABI. C and C++ mods are
tested: the example mods
[`hello.c`](https://github.com/enshwick/enshwick/blob/main/examples/hello-c/hello.c)
and
[`hello.cpp`](https://github.com/enshwick/enshwick/blob/main/examples/hello-c/hello.cpp)
start on the fake host on every change. Any other language that builds a
Windows DLL and can call a C ABI can write a mod the same way.

## The header and the examples

The header is
[`enshwick.h`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/include/enshwick.h),
written from the same Rust types the loader uses and checked against the ABI's
layout on every change. It also compiles as C++20. Each example writes one
line to the log when it starts.

## Write the mod

1. Copy `enshwick.h` next to your source file.
2. Fill a static `EnshwickManifest`. Its `header` is the struct's size and the
   ABI version. The version is `ENSHWICK_ABI_MAJOR` and `ENSHWICK_ABI_MINOR`.
   A mod needs at least `id` and `version`. It needs `side` too.
   `ENSHWICK_STR_INIT("text")` makes a string.
3. Define the four functions the DLL exports:
   - `enshwick_mod_abi`
   - `enshwick_mod_query`
   - `enshwick_mod_init`
   - `enshwick_mod_ready`

   The header declares them with `ENSHWICK_EXPORT`, so the DLL exports them.
4. In `enshwick_mod_init`, call the loader through the `EnshwickHostApi` table
   it passes. An example is `host->log(me, EnshwickLevel_INFO, line)`.

Names follow one pattern:

| Rust item                | C name               |
| ------------------------ | -------------------- |
| `Status::OK`             | `EnshwickStatus_OK`  |
| the type `HookSpec`      | `EnshwickHookSpec`   |
| a constant: `MAX_ZONES`  | `ENSHWICK_MAX_ZONES` |

Every comment in the header is the Rust item's documentation.

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

A C++ mod builds the same way with C++20, which allows the designated
initializers the manifest uses:

```sh
clang++ --target=x86_64-pc-windows-msvc -std=c++20 -O2 -shared -fuse-ld=lld \
  hello.cpp -o hello_cpp.dll
```

The header wraps the four exports in `extern "C"`, so their names stay
unmangled. `hello.cpp` turns off exceptions and runtime type information
(`-fno-exceptions -fno-rtti`) to build without the C++ runtime as well.
A mod that uses the C++ standard library links its runtime as usual.

The loader loads your mod with
[`LoadLibraryExW`](https://learn.microsoft.com/en-us/windows/win32/api/libloaderapi/nf-libloaderapi-loadlibraryexw).
It passes the flags `LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR` and
`LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`. So a DLL your mod links against loads
only from your mod's folder, the game's folder or the Windows system folder.
It never loads from the current directory or `PATH`. Ship every DLL it links against
beside it, or link the C runtime statically (`/MT`). A DLL your mod loads
itself, or marks for delay loading, follows
[the default search order](https://learn.microsoft.com/en-us/windows/win32/dlls/dynamic-link-library-search-order).
That order leaves out your mod's folder, so load such a DLL by its full path.

## Check it

Run `cargo enshwick manifest hello_c.dll` on Windows. It loads the DLL, asks it
for its manifest and prints the id, version and side. Then copy the DLL into a
folder of its own in the game's `mods/` folder, such as `mods/you.hello-c/`. The log shows `hello from C` under `you.hello-c` when the
loader starts it.

## Keep it current

A loader loads a mod built against an older header while
`ENSHWICK_ABI_MAJOR` is the same. So rebuild the mod when a release's
`ENSHWICK_ABI_MAJOR` differs from the one you built with, against that
release's `enshwick.h`.
`enshwick_mod_abi` returns `ENSHWICK_ABI_MAJOR`. The
[ABI reference](abi-reference.md#versions) has the rules.
