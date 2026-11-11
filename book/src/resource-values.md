<!--
type: how-to
description: Read any resource of the game's archive as typed values by its reflected type name, and serve a changed copy when the game reads it, for every resource type.
-->

# Game resources as values

`enshwick::resources` reads any resource of the game's archive as typed values
and serves your change when the game reads it. It goes by the game's reflected
type names, so it reaches every resource type.

## Find a type

[`Registry::game`] holds every reflected type of the running build. Look one
up by its qualified name, such as `keen::BalancingTable`, or by its impact name
or either name's hash. A type tells its size, kind, fields with their offsets
and attributes, enum constants and base.

## List and read resources

[`Host::resource_keys`] lists the keys of every resource of a type, or of every
resource, in archive order. [`Host::game_value`] reads one as a [`Value`]. Each
kind of data reads as follows:

- a struct is its fields by name;
- an enum constant is its name;
- a bitmask is the names of its set flags;
- a GUID or reference is its GUID;
- an absent optional is `Nil`.

[`Host::content_hashes`] and [`Host::game_content`] do the same for contents.

## Serve a change

Decode the base in [`Load::serve`], change what you need, and encode it over
the base. The codec keeps every byte whose value did not change. Because of
that, other mods' changes to the same resource chain in load order, as with any
served resource.
A struct you encode may name only the fields it changes.

For a resource of your own, declare its id with `Load::declare`. Then serve a
value under that GUID with `Load::serve_new`. The value alone makes the
resource, and every field the value leaves out is zero. The game must hold a resource of that
type, which the loader opens as the base the value replaces.

```rust
use enshwick::resources::{Registry, Value};

const TABLE: &str = "keen::BalancingTable";

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let registry = Registry::game()?;
    let ty = registry.named(TABLE).ok_or_else(|| enshwick::Off::new("no balancing table"))?;
    for key in load.host().resource_keys(Some(ty))? {
        load.serve(key, key, move |base| {
            let mut table = registry.decode(TABLE, base).ok()?;
            if let Some(Value::Float(factor)) = table.field_mut("damageMod2Handed") {
                *factor *= 2.0;
            }
            registry.encode(TABLE, &table, Some(base)).ok()
        })?;
    }
    Ok(())
}
```

```luau
for _, key in enshwick.resources.resource_keys("keen::BalancingTable") do
    enshwick.resources.serve(key, key, function(table)
        return { damageMod2Handed = table.damageMod2Handed * 2 }
    end)
end
```

`damageMod2Handed` is 1.5 on dedicated server build 26319703.

An array that grows moves to the end of the resource, its offset rewritten.
Elements match by position. After you remove one, each later element is written
over the bytes of the element that held its index before. As a result, fields
the type does not reflect come from that element.
An array whose data is packed, such as a voxel grid, reads as its header's
bytes and writes back unchanged.

## From Luau

`enshwick.resources` has the same calls:

- `named` and `hashed`;
- `impact_named` and `impact_hashed`;
- `type_names` and `resource_keys`;
- `content_hashes` and `game_content`;
- `serve_content` and `game_value`;
- `decode` and `encode`;
- `serve` and `serve_new`;
- `declare` and the GUID and hash helpers.

A type the build lacks is `nil`. A value is a plain table; a variant
is `{ type = ..., value = ... }`
and raw bytes are a `buffer`. An integer past 2^53, which a Luau
number cannot hold exactly, is its decimal text, and text is taken back for any
integer. An absent optional or variant is a field the table leaves out, and
`enshwick.resources.none()` as an item of a list, which `nil` would cut short.
A field an edit leaves out keeps its value, so give `none()` to clear one. A patch returns
the table to serve, naming only what it changes, or `nil` to serve the base
unchanged. A patch that fails, or returns a value that does not fit the type,
is logged. A game resource patched under its own key is then served
unchanged; a new resource turns the mod off. The Luau tab of
[Serve a change](#serve-a-change) shows a patch.

[`Registry::game`]: https://github.com/enshwick/enshwick/blob/main/crates/internal/base/src/resources/table.rs
[`Host::resource_keys`]: https://github.com/enshwick/enshwick/blob/main/crates/internal/base/src/resources/table.rs
[`Host::game_value`]: https://github.com/enshwick/enshwick/blob/main/crates/internal/base/src/resources/table.rs
[`Host::content_hashes`]: https://github.com/enshwick/enshwick/blob/main/crates/internal/base/src/resources/table.rs
[`Host::game_content`]: https://github.com/enshwick/enshwick/blob/main/crates/internal/base/src/resources/table.rs
[`Value`]: https://github.com/enshwick/enshwick/blob/main/crates/internal/values/src/resources/value.rs
[`Load::serve`]: https://github.com/enshwick/enshwick/blob/main/crates/internal/content/src/content.rs
