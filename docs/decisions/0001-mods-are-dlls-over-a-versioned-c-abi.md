# 0001: Mods are DLLs over a versioned C ABI

Accepted, 2026-10-20.

In the context of a loader meant to become the community standard, facing mod
authors who build with different compiler versions and release on their own
schedule, we decided for one DLL per mod talking to the loader over a `repr(C)`
interface with a size-and-version header, and against Rust dylibs or mods
compiled into the loader, to achieve independent releases and a contract that
survives compiler and loader updates, accepting hand-kept layouts and a safe
layer to hide them.

## Context

Rust has no stable ABI: a `dylib` or a trait object only works between binaries
built by the same compiler with the same flags. A mod ecosystem needs mods built
months apart, by people the project does not coordinate, to load into the same
loader. The loader also has to read a mod's manifest before it runs any of the
mod's code, and turn one mod off without touching the others.

## Decision

Every mod is a `cdylib` that exports `enshwick_mod_abi`, `enshwick_mod_query`,
`enshwick_mod_init` and `enshwick_mod_ready`, and everything crossing the
boundary is `repr(C)` with `extern "system"` functions, as defined in
`enshwick::abi`.

## Considered options

| Option                        | Good                                                                                               | Bad                                                                 |
| ----------------------------- | -------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------- |
| DLL per mod over a C ABI      | any compiler, any release cadence; a manifest is read without running code; one mod can be skipped | layouts are kept by hand; a safe layer is needed                    |
| Rust `dylib` per mod          | plain Rust types across the boundary                                                               | breaks with every compiler or flag change; no ecosystem survives it |
| Mods compiled into the loader | no boundary at all                                                                                 | one release for all mods; no third-party mods                       |
| Scripting language for mods   | sandboxing                                                                                         | hooks on hot game functions need native speed; a second toolchain   |

## Consequences

Mods and the loader release independently. Structs only grow at the end, and a
mod checks `Header::covers` before using a newer field. Strings are borrowed for
the call and memory is never freed across the boundary. The safe layer in
`enshwick` carries the ergonomic cost.

## Confidence

High. A real need for in-process scripting, or a stable Rust ABI, would reopen
it.

## Compliance

Compile-time layout assertions in `enshwick::abi`; `improper_ctypes` and
`improper_ctypes_definitions` denied; the lifecycle test drives a mod through a
fake loader.
