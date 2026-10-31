# Concepts

These pages say how the loader and a mod work together. Read them once
before the chapters on writing a mod; each is short.

## The pages

- [The loader and the ABI](concepts/loader-and-abi.md): how the loader starts,
  how it finds mods, the binary contract between them, and the manifest.
- [The life of a mod](concepts/lifecycle.md): query, resolve, init, arm and
  ready, and what happens when a mod fails.
- [Sides and hosting](concepts/sides-and-hosting.md): the dedicated server,
  the client, and the server a client runs while it hosts a game.
- [The join check](concepts/join-check.md): how a server and its players agree
  on a mod set.
- [Threads](concepts/threads.md): which thread runs what, and what may be
  called where.
