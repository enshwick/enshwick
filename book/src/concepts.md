<!--
type: explanation
description: How the Enshwick loader and a mod work together: the ABI, the life of a mod, sides and hosting, the join check, threads and typed types across game builds.
-->

# Concepts

These pages say how the loader and a mod work together. Read them once
before the chapters on writing a mod; each is short.

## The pages

- [The loader and the ABI](concepts/loader-and-abi.md): how the loader starts,
  how it finds mods, the binary contract between them, and the manifest.
- [How ids are namespaced](concepts/ids.md): the mod id, the full ids every
  name a mod declares gets, and what the loader refuses.
- [The life of a mod](concepts/lifecycle.md): query, resolve, init, arm and
  ready, and what happens when a mod fails.
- [Sides and hosting](concepts/sides-and-hosting.md): the dedicated server,
  the client, and the server a client runs while it hosts a game.
- [The join check](concepts/join-check.md): how a server and its players agree
  on a mod set.
- [Threads](concepts/threads.md): which thread runs what, and what may be
  called where.
- [Typed types across game builds](concepts/typed-stability.md): what a game
  update does to the game's types a mod uses.
- [Performance](concepts/performance.md): what native and Luau mods cost at
  run time.
