<!--
type: explanation
description: Why a game update needs no Enshwick update: the loader reads the game's own description of its data at every start, and finds the rest by what the engine names.
-->

# Why a game update needs no Enshwick update

Enshwick reads the game's own description of its data at every start. When an
update moves a field, Enshwick finds it in its new place. A mod built on the
API will most likely keep working.

## What does the game say about itself?

The game ships a description of its own data types: their names, their fields,
where each field lies and how big each type is. This description is the game's
reflection data.

The loader reads it at every start. It keeps no copy of its own for a game
version, so there is nothing to update when the game moves a field.
[Finding the game](../finding-the-game.md#reflection-systems-fields-and-sizes)
shows the lookups a mod makes through it.

## How does the loader find the game's functions?

There is no table per game version. The loader finds the functions it needs
through what the engine itself names: its ECS systems and its interface's debug
strings. The few entries no such route reaches have an
anchor, a short code pattern that found the entry on every build it was checked
on. A new build can still break a route or an anchor.

Both kinds are data built into the loader. The loader searches on every start
and writes nothing. [Compatibility](https://enshwick.com/reference/compatibility/)
names the game version the newest release was checked on.

## What happens to what the loader cannot find?

Only the features that need the missing part turn off, and everything else
keeps working. The log names the game build and what is missing. Players see
the reason in chat and on the Mods screen. A mod keeps its other features and
the other mods load.

## How does the typed API follow the game?

The items of `enshwick::typed` are generated from the game's reflection data,
for every component, event and resource it describes. A new game build gets
its items from a new run of the generator, so the API grows with the game.

An item reads and writes a value by the game's field names, never by a stored
offset. Before it reads, it checks the type's shape on the running build. A
field that moved reads as before. A field the build lacks turns off that type
alone. [Typed types across game builds](typed-stability.md) lists each kind of
change.
