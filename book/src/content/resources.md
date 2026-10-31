# Served resources

Items, recipes and workshops are built on one lower-level call,
[`Load::serve`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content.rs).
It serves any game resource, new or changed, as if it came from the game's
archive. Reach for it when the high-level calls do not cover what you need.

## Keys and bases

A resource is named by a
[`ResourceKey`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/abi.rs):
a GUID, a type hash and a part.
[`Guid::key(ty)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content.rs)
makes the key of part 0 with the hash the loader gives the type on the
running build, and
[`content::types`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content.rs)
names the resource types mods commonly serve, such as `ITEM_INFO` and
`ITEM_REGISTRY`.

`load.serve(key, base, builder)` serves `key`, built by `builder` from the
bytes of the resource `base`:

- with `key` and `base` different, it is a new resource, such as a new item
  cloned from a vanilla one;
- with them equal, it is a changed vanilla resource, such as the item
  registry with one more entry.

```rust
use enshwick::content::{Blob, Guid, Id, id, types, vanilla};

const LOCKER_ID: Id = id!("locker");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    const LOCKER: Guid = LOCKER_ID.guid();
    let storage_box = vanilla::item("Prop_Storage_24_T1_Box")?;
    load.declare(LOCKER_ID)?;
    load.serve(LOCKER.key(types::ITEM_INFO)?, storage_box.key(types::ITEM_INFO)?, |base| {
        let mut item = Blob::new(base);
        // Offsets for illustration: a real mod reads them from reflection.
        let written = item.write(0, &enshwick::ItemId::from_guid(LOCKER).get().to_le_bytes()) && item.write(4, &LOCKER.0);
        written.then(|| item.into_bytes())
    })
}
```

Field offsets change between game builds; resolve a layout such as
[`ItemLayout::resolve`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/items.rs)
during `init`, which asks the loader by Enshwick's ids, and move it into
the builder. For a field no layout names, `load.field_offset(type, field)`
and `load.type_size(type)` read the game's reflection. See
[Finding the game](../finding-the-game.md).

## When the builder runs

`serve` only registers the builder; call it during `init`. The game builds
the resource the first time it reads it, on whatever thread it reads it, and
keeps the result for the life of the process. So the builder:

- is `Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync + 'static`: move what it
  needs into it;
- runs once per resource, not per read, unless the mod rebuilds it;
- returns `None` when the base does not read as expected. That turns the mod
  off. A panic in the builder counts the same.

When the game has no `base` in its archive, the served key stays missing and
the builder never runs.

## Rebuilding

When values a builder reads change after the game built the resource, such
as a server's join values on a joined client, the mod can have it built
again: `load.allow_rebuild(key)` during `init`, after `serve`, has the loader
keep a copy of the base, and `content::rebuild(key)` later, from any thread,
runs every mod's patch on that copy and writes the bytes that differ over
the resource the game holds. Nothing happens while the game has not built
the resource yet. The game keeps reading the first build's bytes, so a
rebuild must come out the same length: changed values qualify, added
entries do not, and `rebuild` returns `Err(Off)` for them. Each rebuild
keeps the new bytes for the process, so it is for rare changes.

## Patches from several mods compose

Several mods may serve one key. Each builder is a patch, and the loader runs
the patches in load order, each on the result of the one before. This is how
two mods both add an item to the item registry: each builder sees the
registry with the earlier mods' items already in it, and adds its own.
Write builders to expect that: find entries by what they are, never by a
fixed index or count.

The rules:

- every mod serving one key must name the same `base`; a mod that names
  another base for a key already served is refused, and `serve` returns
  `Err(Off)`;
- a patch that fails turns its mod off and is skipped; the next patch runs
  on the result so far;
- when every patch fails, the resource reads as missing;
- the patches of a mod already turned off are skipped.

At the `debug` level the loader's log names each built resource and the mods
that built it:
`served resource <type>:<guid> built by you.lockers, you.lanterns, <n> bytes`.
The [status summary](../logging.md) counts what mods serve, what was built
and what failed.

## Editing blobs

Game resources are blobs: plain data whose arrays and strings are offsets
relative to the field that holds them. Growing an array in place would break
every offset after it.
[`content::Blob`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content.rs)
edits a blob safely, by moving a grown array to the end and correcting the
offsets that move with it.

| Method | What it does |
| --- | --- |
| `Blob::new(base)` | A copy of `base` to edit. |
| `bytes()`, `into_bytes()` | The bytes as edited, borrowed or owned. |
| `read_u32(at)` | The `u32` at `at`, `None` past the end. |
| `write(at, bytes)` | Writes `bytes` at `at`; `false` when they do not fit. |
| `array(field)` | The array whose header is at `field`: where its elements start and how many there are. |
| `set_array(field, start, count)` | Points the header at `field` to `count` elements at `start`. |
| `append(data, align)` | Appends `data` at the next multiple of `align` and returns where. |
| `grow_array(field, size, align, relative, extra)` | Moves the array at `field`, of `size`-byte elements, to the end with `extra` elements after the old ones; returns the index of the first extra one. |

An array or string header is 8 bytes: a signed 32-bit offset from the header
itself, then a 32-bit count.

`grow_array` takes two lists that keep offsets right:

- `relative` lists the offsets, inside one element, of the element's own
  array and string headers. As elements move, their headers are corrected
  so they still point at their data;
- each extra element comes with `Some(offset)` when its bytes were copied
  from that offset in the blob, so its headers are corrected the same way,
  or `None` when its headers are empty or set later with `set_array`.

```rust
use enshwick::content::Blob;

// A root with one array header at 0: two u32 elements right after it.
let mut base = vec![8, 0, 0, 0, 2, 0, 0, 0];
base.extend_from_slice(&[1, 0, 0, 0, 2, 0, 0, 0]);
let mut blob = Blob::new(&base);

let first = blob.grow_array(0, 4, 4, &[], &[(vec![3, 0, 0, 0], None)]);
assert_eq!(first, Some(2));
let (start, count) = blob.array(0).unwrap();
assert_eq!(count, 3);
assert_eq!(blob.read_u32(start + 8), Some(3));

// A string for a new element: append the text, then point a header at it.
let text = blob.append(b"Locker\0", 1);
let header = blob.append(&[0; 8], 4);
assert!(blob.set_array(header, text, 4));
assert_eq!(blob.array(header), Some((text, 4)));
```

The API's own
[`items.rs`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/items.rs),
[`recipes.rs`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/recipes.rs)
and
[`workshops.rs`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content/workshops.rs)
are worked examples: each builds its resources with `Blob` alone.

## Reading vanilla resources

A builder sees only its own base. To read another vanilla resource, such as
the template a station item places or an item's name tag,
[`content::game_resource(key)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content.rs)
returns its bytes from the game's archive files, as the game reads them
before it resolves their references, so `Blob` reads them the same way. It
works during `init` and later, on any thread and on both sides, and never
touches the game's memory. Each call reads the archive, so keep what a mod
needs more than once.

```rust
use enshwick::content::{Blob, game_resource, types};
use enshwick::content::workshops::stations;

let key = stations::WORKBENCH.item.key(types::ITEM_INFO);
if let Some(item) = key.ok().and_then(game_resource) {
    let item = Blob::new(&item);
    let _id = item.read_u32(0);
}
```

## One text for one tag

[`Load::add_text(tag, language, text)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content.rs)
is the low-level text call: the client shows `text` wherever the game asks
for the `LocaTag` `tag`, a declared [content id](../content.md#content-ids)'s
GUID, in `language`, a
[`Language`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/i18n.rs)
number. A served item whose name field holds `tag` shows that text as its
name.

```rust
use enshwick::content::{Id, id};
use enshwick::i18n::Language;

const LOCKER_NAME: Id = id!("locker.name");

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let tag = load.declare(LOCKER_NAME)?;
    load.add_text(tag, Language::FALLBACK, "Locker")?;
    load.add_text(tag, Language::DE_DE, "Tresor")
}
```

Most mods use [`add_texts`](texts.md) instead, which gives many tags their
texts in every language from the mod's texts file.
