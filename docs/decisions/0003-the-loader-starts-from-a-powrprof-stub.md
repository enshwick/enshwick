# 0003: The loader starts from a powrprof.dll stub

Accepted, 2026-10-20.

In the context of a loader that must start inside both the game and its
dedicated server without a launcher, facing other loaders that already use the
usual proxy names, we decided for a small `powrprof.dll` stub that forwards the
system copy and loads `mods/enshwick/enshwick.dll`, and against a single proxy
DLL or a common name such as `dinput8.dll`, to achieve coexistence with other
loaders and a loader that can be updated while the stub stays put, accepting one
more file to install.

## Context

Windows loads a DLL the game imports from the game's folder first, so a proxy of
such a DLL runs inside the game. Both binaries import `powrprof.dll`,
`dinput8.dll`, `dbghelp.dll` and others. `dinput8.dll` is the usual name for
loaders such as Ultimate ASI Loader, and EML uses `dbghelp.dll`. A DLL that is
loaded cannot replace itself, which an updater needs.

## Decision

The proxy is `powrprof.dll`, a stub that only forwards exports and loads the
loader from `mods/enshwick/enshwick.dll`, which does everything else.

## Considered options

| Option                              | Good                                                               | Bad                      |
| ----------------------------------- | ------------------------------------------------------------------ | ------------------------ |
| `powrprof.dll` stub plus loader DLL | no clash with other loaders; the loader can be swapped by the stub | two files                |
| One `powrprof.dll` with everything  | one file                                                           | cannot update itself     |
| `dinput8.dll` proxy                 | the name players know from other games                             | clashes with ASI loaders |
| `dbghelp.dll` proxy                 |                                                                    | taken by EML             |

## Consequences

Enshwick runs beside EML or an ASI loader. Updates replace the loader and mods
while the stub, which rarely changes, stays. Linux players set
`WINEDLLOVERRIDES="powrprof=n,b"` under Proton.

## Confidence

High. A game update that stops importing `powrprof.dll` would reopen it.

## Compliance

The stub crate builds `powrprof.dll` with exactly the system DLL's exports.
