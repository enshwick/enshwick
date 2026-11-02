# ABI reference

The loader and a mod talk through a C ABI. Mods normally use the safe layer
of `enshwick`; this chapter is for reading logs and the rare call to the raw
table.

## The boundary

The ABI is made of `repr(C)` structs, integer enums, `extern "system"`
functions, strings borrowed for the call, and no unwinding across the
boundary. The exact types and signatures are in the
[`abi` module's rustdoc](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs). The same types for
C and other languages are in `enshwick.h`, see [Mods in C](mods-in-c.md).

## Versions

The layout file
[`abi/layout.txt`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi/layout.txt)
lists the layout of every ABI type, field by field: offset, size and type,
and for a function pointer its signature. Its first line is the ABI
version, a major and a minor:
[`ABI_MAJOR` and `ABI_MINOR`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs).

| Change to the layout                                          | Version                |
| ------------------------------------------------------------- | ---------------------- |
| A field at the end of a struct that starts with a `Header`    | the next minor         |
| A new type, or a function at the end of `HostApi`             | the next minor         |
| Anything else: a removed, moved or retyped field, a new order | a new major at minor 0 |

A loader loads a mod built for its own major and its own minor or an older
one:

- A mod exports `enshwick_mod_abi`, the major it was built for. A loader of
  a newer major does not load it and logs
  `<file> skipped: built for ABI major 4, and Enshwick 1.0.0-alpha.12 runs ABI 5.0: rebuild it for Enshwick 1.0.0-alpha.12`;
  a loader of an older major logs that the mod is newer and asks to update
  Enshwick.
- The manifest's header holds the minor. A loader older than that minor
  logs `<file> skipped: built for ABI 5.3, newer than ABI 5.1 of Enshwick 1.0.0-alpha.12: update Enshwick`.
- Structs that cross the boundary on their own start with a
  [`Header`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs):
  the struct's size as its writer knew it and the writer's major and minor.
  A reader takes a field only when
  [`Header::covers`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs)
  it, so a mod of an older minor and a loader of a newer one read the
  fields both know.

A mod keeps loading across every release of its major, so a mod author
rebuilds only for a new major. Pin `enshwick` with `=` in a mod's
`Cargo.toml`, such as `enshwick = "=1.0.0-alpha.12"`, as `cargo enshwick new`
writes it; a caret requirement lets `cargo update` move the mod to a
release of a newer minor, which older loaders refuse.

CI builds the rolling `dev` loader from the newest commit of `main`. To load
under it, build the mod against `main` from git, pinned to a commit:

```toml
[dependencies]
enshwick = { git = "https://github.com/enshwick/enshwick", rev = "<commit>" }
```

The mod loads while the `dev` loader has that commit's major and its minor or
a newer one.

## How does `enshwick` version its API?

The `enshwick` crate follows [SemVer](https://semver.org) for its Rust API.
Until 1.0.0 the crate is a prerelease: any release may break the API and
the ABI, and a break raises the prerelease number of `1.0.0-alpha.N`.

The repository checks this in CI:

- A test fails when the layout of the ABI types differs from
  `abi/layout.txt`, or when `ABI_MAJOR` and `ABI_MINOR` are not the
  version the change needs; it names the version to set.
- CI runs [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks)
  on `enshwick` against its last release on crates.io, so a release whose
  version claims less change than its API made fails.

## The exports of a mod

[`declare_mod!`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/declare.rs)
writes five exports:

| Export                  | Signature                                 | When the loader calls it                               |
| ----------------------- | ----------------------------------------- | ------------------------------------------------------ |
| `enshwick_mod_abi`      | `fn() -> u32`                             | First: the ABI major the mod was built for.            |
| `enshwick_mod_query`    | `fn() -> *const Manifest`                 | Before any of the mod's code runs; static data.        |
| `enshwick_mod_init`     | `fn(*const HostApi, ModHandle) -> Status` | Once, in load order; registers hooks and the rest.     |
| `enshwick_mod_ready`    | `fn(*const HostApi, ModHandle)`           | After every chain is armed, if `init` returned OK.     |
| `enshwick_mod_shutdown` | `fn()`                                    | Before a development reload unloads it; optional.      |

A DLL in `mods/` without the first four is logged as `<file> is not an Enshwick mod`
and unloaded.

## The manifest

[`Manifest`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs)
is what [`declare_mod!`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/lib.rs)
builds from the crate's `Cargo.toml`; the rustdoc of `abi::Manifest` lists its
fields, and [the manifest](concepts/loader-and-abi.md#the-manifest) the keys
they come from.

The loader skips a mod whose manifest has another major or a newer minor, an invalid id, the
loader's own id `enshwick`, a version that is not SemVer, an unknown side, or a
dependency range that does not parse; the reason goes to `loader.log` as
`<file> skipped: <reason>`.

## Status codes

Every call that can fail returns a
[`Status`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs).
The safe layer turns a failed status into an
[`Off`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
reason of the form `<what>: <why>`, such as
`symbol quest.reward: missing on this game build`:

| Code | Constant        | Meaning                                                                    | `<why>` in the Off reason          |
| ---- | --------------- | -------------------------------------------------------------------------- | ---------------------------------- |
| 0    | `OK`            | Success                                                                    |                                    |
| 1    | `MISSING`       | What was asked for does not exist on this game build                       | `missing on this game build`       |
| 2    | `AMBIGUOUS`     | More than one candidate answers a lookup                                   | `ambiguous on this game build`     |
| 3    | `REFUSED`       | Conflicts with another mod or with the loader's rules                      | `refused by the loader`            |
| 4    | `BAD_ARGS`      | An argument is malformed                                                   | `malformed`                        |
| 5    | `TOO_LATE`      | Called in a phase where it is not allowed, such as a hook after arming     | `asked for too late`               |
| 6    | `FAULTED`       | The mod has faulted and is off                                             | `unavailable: the mod has faulted` |
| 7    | `WRONG_THREAD`  | Called from a thread the function does not serve                           | `called off the thread it serves`  |
| 8    | `NOT_LOADED`    | The part of the world asked about is not loaded yet; try again in a second | `not loaded yet`                   |
| 9    | `OUTSIDE_WORLD` | The box asked about leaves the world, past its edge or above its height    | `outside the world`                |
| 10   | `STATIC_HALF`   | The static half of a component type: template data, never written          | `template data, never written`     |
| 11   | `NOT_A_PROP`    | The entity is no prop: a creature, a flame altar or not placed             | `not a prop`                       |
| 12   | `NOT_KEPT`      | Nothing finds the entity after a load: a player, or no place in the world  | `not found again after a load`     |

`enshwick_mod_init` itself returns `OK`, `REFUSED` when `Mod::init` returned
`Err(Off)`, or `FAULTED` when it panicked; the loader logs
`off: init returned 3` or `off: init returned 6` under the mod's id.

Every host function answers a mod that is off with `FAULTED`, and a handle
the loader never gave out with `BAD_ARGS`. A panic in the loader's own code
behind a host function never reaches the mod: the call returns `REFUSED`
(or zero for a function that returns a value) and the loader logs the
fault.

## Ids

Struct fields and host function arguments that name a thing of the game
take an id type, the same size and alignment as its integer. The header
declares each as a typedef:

| Rust        | C                   | Integer | Names                                       |
| ----------- | ------------------- | ------- | ------------------------------------------- |
| `Entity`    | `EnshwickEntity`    | `u32`   | an entity of the world; 0 for none          |
| `ItemId`    | `EnshwickItemId`    | `u32`   | an item, by the number its item data gives  |
| `PlayerKey` | `EnshwickPlayerKey` | `u64`   | a player's account, the same every session  |
| `RecipeId`  | `EnshwickRecipeId`  | `u32`   | a crafting recipe                           |

## Recipe kinds

`HostApi::recipe_kind` answers a
[`RecipeKind`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi/content.rs)
code, and a recipe selection of `change_data` and a recipe output `RateId`
take one. A mod reads a code it does not know as `OTHER`.

| Code | Constant            | The item a recipe makes                                                                      |
| ---- | ------------------- | -------------------------------------------------------------------------------------------- |
| 0    | `OTHER`             | none of the kinds below                                                                      |
| 1    | `BUILDING_BLOCK`    | equipment slot `BlueprintMaterial_Default`                                                   |
| 2    | `ROOF`              | equipment slot `BlueprintMaterial_Roof`                                                      |
| 3    | `OVERGROWTH`        | equipment slot `OvergrowthMaterial`                                                          |
| 4    | `TERRAIN`           | equipment slot `TerrainMaterial`                                                             |
| 5    | `DECORATIVE`        | slot `BuildTool` with the Decorative menu group's item tag                                   |
| 6    | `COMFORT_FURNITURE` | slot `BuildTool` with the Comfort Furniture menu group's item tag and not the Decorative one |

## Events

A mod subscribes with
[`Load::on`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/load.rs)
and the types in
[`events`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/events.rs),
or with a raw [`Event`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs)
id through `Load::on_raw_event`. See [Events and the scheduler](events.md).

`on_raw_event` takes the event's id, an `abi::Event` constant such as
`Event::TICKED` (3), and hands the handler the data pointer, null for events
without data. The rustdoc of `abi::Event` lists every id, and
[Event kinds](events.md#event-kinds) has each event's data, time and thread.

## The host table

[`HostApi`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi/host_api.rs)
is the table of functions the loader passes to `enshwick_mod_init`: every
service a mod reaches, from logging and hooks to the world, content and
other mods. The safe layer wraps each field in a typed call. A field whose
documentation says it is allowed during `enshwick_mod_init` only refuses
later calls. The rustdoc of `abi::HostApi` lists every field with
its threads and statuses, and `enshwick.h` has the same comments for C as
`EnshwickHostApi`.
