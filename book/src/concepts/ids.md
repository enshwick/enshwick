<!--
type: reference
description: Every id a mod declares goes under the mod's own id. The forms, the limits, naming other mods' and the game's ids, and what the loader refuses.
-->

# How ids are namespaced

Every name a mod declares belongs to that mod. The full id puts the mod's id
in front, so two mods never share a name.

## What does a mod id look like?

A mod id is `<author>.<name>`, such as `you.trading-post`:

- each part is one or more of `[a-z0-9-]`;
- there is exactly one `.`;
- the whole id is at most 64 characters.

The manifest sets it, see [the manifest](loader-and-abi.md#the-manifest). An
invalid id fails the build. Two ids have no dot, so no mod can take them:

| Id | Belongs to |
| --- | --- |
| `enshwick` | the loader, and its built-in stat kinds |
| `enshrouded` | the game's own content, where a person writes a name |

## What does a full id look like?

A full id is `<mod id>:<name>`, joined by one colon. The mod writes only the
name, and the platform puts its id in front. The name of a content id is 1
to 128 of `[a-z0-9_./-]`, so ids read like paths:

- `you.lockers:locker`
- `you.lockers:locker.name`
- `you.market-stalls:stall/blacksmith`

A second colon makes a content id invalid. A stat kind or affix name is at
most 123 characters, since its text key adds `.name`.

## Which names get the prefix?

| Kind | Full id | The mod writes | Name rule |
| --- | --- | --- | --- |
| Content | `<mod>:<name>` | `id!("locker")`; a pack's name | 1 to 128 of `[a-z0-9_./-]` |
| Stat kinds | `<mod>:<name>` | `id!("luck")` | as content, at most 123 |
| Affixes | `<mod>:<name>` | `id!("sturdy")` | as content, at most 123 |
| Services, events, points, calls | `<mod>:<name>` | `name = "price"` | not empty; as content when exported or from Luau |
| Item and entity data kinds | `<mod>:<name>` | `"charge"` | 1 to 64 of `[a-z0-9_-]` |
| Controls | `<mod>:<name>` | `Control::new("horn", ..)` | 1 to 32 of `[a-z0-9-]` |

Content covers every game resource a mod adds. That includes items, recipes,
workshops, templates and icons. It also includes text tags, buffs, sounds and
weather phenomena.

Three kinds are scoped by the mod without a colon:

| Kind | Scope | Name rule |
| --- | --- | --- |
| [Actions](../actions.md) | the console line `<mod id> <action>` | 1 to 32 of `[a-z0-9-]` |
| [Settings](../settings.md) | the folder `<stem>-mod-settings/<mod id>/` | a file name of `[a-z0-9_-]`, `settings` by default |
| [Texts](../content/texts.md) | the mod's own texts file | a key of that file |

A text the game shows is a content id: the texts file maps its key to a tag
such as `you.lockers:locker.name`. Luau adds the prefix to calls, points and
events. A Luau script writes content, stat kinds and affixes in full.

## How does a content id become a GUID?

The game knows content by GUID, so each content id turns into one. It is a
version 5 (name-based, SHA-1) UUID of the id's UTF-8 bytes. The namespace is
[`NAMESPACE`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/ids.rs),
`e2185889-0e5e-4977-9edd-d6e7bcd1393f`.

The same id gives the same GUID on every machine and in every release:

- Python's
  `uuid.uuid5(uuid.UUID("e2185889-0e5e-4977-9edd-d6e7bcd1393f"), "you.lockers:locker")`
  prints `67842cb8-d551-542b-8765-fe1af9c28aca`;
- `cargo enshwick id you.lockers:locker` prints it with its game id.

Saves hold these GUIDs, so a content id is fixed once players have the
content. To move content to another id, see
[renamed content](../content.md#renamed-content). Stat kinds and affixes take
their keys from the same GUIDs.

## How does a mod name its own ids?

[`id!`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/ids.rs)
puts the mod's id in front of a name. It reads the id from
`[package.metadata.enshwick]` in the crate's `Cargo.toml`. `part` derives the
ids of one piece of content from its name. Both are constants, and a
malformed name fails the build:

```rust
use enshwick::content::{Guid, Id, id};

// In the mod `you.lockers`, this is `you.lockers:locker`.
const LOCKER: Id = id!("locker");
const LOCKER_NAME: Id = LOCKER.part("name");

assert_eq!(LOCKER_NAME.to_string(), format!("{LOCKER}.name"));
assert_eq!(LOCKER.guid(), Guid::named(&LOCKER.to_string()));
```

A call, point, service or event takes a local name: `name = "price"` is the
mod's own `<id>:price`. A name with a `:` stays as written, for tests and
tools.

## How does a mod name another mod's or the game's ids?

| Where | Another mod's | The game's |
| --- | --- | --- |
| Rust, content | `deps::<alias>::id("locker")` | `deps::vanilla::item("Prop_Production_T1_Forge")` |
| Rust, a stat kind | `deps::<alias>::stat("luck")` | the built-in kinds of `enshwick::stats` |
| Rust, a call, point, service or event | `of = <alias>` on its declaration | none |
| A data pack | `<alias>:<name>` or `<mod>:<name>` | a debug name without a colon |
| A file of disabled recipes | `<mod>:<name>` | `enshrouded:<debug name>` |
| Content, anywhere | the GUID | the GUID |

The alias comes from the manifest's `requires` or `after`, see
[the manifest](loader-and-abi.md#the-manifest). A data pack takes it from
`requires` only, see [data packs](../content/packs.md). Another mod's name passes
only when that mod [exports](loader-and-abi.md#what-other-mods-may-use) it.
Item and entity data kinds are the exception. They are shared by design, and
any mod opens any kind by its full id, see
[item and entity data](../item-and-entity-data.md).

`Id::new("you.lockers:locker")` and `Guid::named("you.lockers:locker")` take
an id in full. They name content without depending on its mod, so the loader
checks no export for them. A call that creates content still turns the mod
off when the `Id` lies outside the mod's namespace.

## What does the loader refuse?

| What | Result |
| --- | --- |
| A content id outside the mod's namespace | the mod turns off |
| A content id whose game id another content id has | the mod turns off; the log names both ids and mods |
| A stat kind or affix whose key another of its kind has | the mod turns off; the log names both |
| A stat kind or affix defined under another mod's id | the call returns `Off` |
| A service, event, point or call outside the mod's namespace | the declaration fails with the rule in the log |
| A control whose name the mod declared already | the second control is refused |
| Another mod's name that mod does not export | the asking call fails, see [what other mods may use](loader-and-abi.md#what-other-mods-may-use) |

A content id's game id is a 32-bit hash of its GUID. For two content ids with
one game id, the log reads:

```text
error [you.lanterns] off: content id you.lanterns:lamp of you.lanterns has the key 0x1a2b3c4d of content id you.torches:torch of you.torches; one of them needs another name (E2003)
```

Content ids, stat kinds and affixes have keys of their own. A stat kind may
share its key with a content id.
