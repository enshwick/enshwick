# Typed types across game builds

The items of `enshwick::typed` come from one game build's reflection data, and
a game update can change any type. This page explains what a mod sees in each
case, and how a new build's items come about.

## Why most changes need nothing

An item holds no offsets, sizes or enum numbers. It reads and writes a value
by the game's field and constant names, through the running build's own
reflection. A field that moves or an enum constant that changes its number
reads as before, with no change to the mod.

## What each kind of change does

| The new build                          | What a mod built before it sees                                                   |
| -------------------------------------- | --------------------------------------------------------------------------------- |
| adds a type, field or enum constant    | nothing changes; the constant reads as `Other` until the mod is rebuilt           |
| moves a field or renumbers a constant  | nothing changes                                                                   |
| removes a type or one of its fields    | that type turns off alone, with a warning that names the build and the field      |
| changes a field's type                 | that type turns off alone when the value no longer reads as the item              |
| renames a type with the same layout    | the type resolves under its new name                                              |
| renames a field                        | that type turns off alone, as for a removed field; a rebuilt mod gets the new name |

A type turns off when a handle resolves it at `init`: the handle reads a
zeroed value of the running build's type as its item, and any field the item
needs and the build lacks fails there. The handle returns the reason, and the
mod decides whether to go on without that type.

## How a rename is decided

A rename is data, never a guess. When `cargo enshwick pin` meets a new build,
it compares the two builds' types. A removed type and an added type count as
one renamed type only when their layouts match exactly and neither has another
match; the same holds for a field at one offset with one type. Every other
case counts as a removal and an addition. Renames go into a table beside the
build tables, and the loader resolves an old type's name under its new one.

## How a new build gets its items

`cargo enshwick pin --old <old exe> --new <new exe>` prints what the new build
changed in the covered types. With `--write` it also keeps a snapshot of the
new build's types beside its build table and writes the typed API again. The
report marks what needs a person: a type that changed its kind or size, and a
rename the table cannot decide. A release of the API crate then carries the
new items.

Before 1.0 the items follow the newest build and may change between
releases. From 1.0 a removed field keeps its item, and the item turns off on
the builds that lack it.
