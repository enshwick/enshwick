# Game resources as values

`enshwick::resources` reads any resource of the game's archive as a tree of
typed values and serves your change to the game when it reads the resource.
It works by the game's own reflected type names, so it reaches every resource
type, not only the ones the API wraps.

## Find a type

[`Registry::game`] holds every reflected type of the running build. Look one
up by its qualified name, such as `keen::BalancingTable`, or by its impact name
or either name's hash. A type tells its size, kind, fields with their offsets
and attributes, enum constants and base.

## List and read resources

[`Host::resource_keys`] lists the keys of every resource of a type, or of every
resource, in archive order. [`Host::game_value`] reads one as a [`Value`]: a
struct is its fields by name, an enum constant its name, a bitmask the names of
its set flags, a GUID or reference its GUID, an absent optional `Nil`.
[`Host::content_hashes`] and [`Host::game_content`] do the same for contents.

## Serve a change

Decode the base in [`Load::serve`], change what you need, and encode it over
the base. The codec keeps every byte whose value did not change, so other mods'
changes to the same resource chain in load order, as with any served resource.
A struct you encode may name only the fields it changes.

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

`damageMod2Handed` is 1.5 on dedicated server build 26319703.

An array that grows moves to the end of the resource, its offset rewritten.
An array whose data is packed, such as a voxel grid, reads as its header's
bytes and writes back unchanged.

## From Luau

`enshwick.resources` has the same calls: `named`, `hashed`, `impact_named`,
`impact_hashed`, `type_names`, `resource_keys`, `content_hashes`, `game_value`
and `serve`. A value is a plain table; a variant is `{ type = ..., value = ... }`
and raw bytes are `{ bytes = { ... } }`. An integer past 2^53, which a Luau
number cannot hold exactly, is its decimal text, and text is taken back for any
integer. A patch returns the table to serve, naming only what it changes, or
`nil` to serve the base unchanged.

```lua
for _, key in enshwick.resources.resource_keys("keen::BalancingTable") do
    enshwick.resources.serve(key, key, function(table)
        return { damageMod2Handed = table.damageMod2Handed * 2 }
    end)
end
```

[`Registry::game`]: https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/resources/table.rs
[`Host::resource_keys`]: https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/resources/table.rs
[`Host::game_value`]: https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/resources/table.rs
[`Host::content_hashes`]: https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/resources/table.rs
[`Host::game_content`]: https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/resources/table.rs
[`Value`]: https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/resources/value.rs
[`Load::serve`]: https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/content.rs
