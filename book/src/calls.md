# Calls between mods

A **call** is a described service: one function from an input struct to an
answer. One mod serves it and other mods call it. The input is a `repr(C)`
struct, so the server's schema describes the call whole. A mod in any language
the loader runs reaches it from that schema. A [service](services.md) table of
several functions stays the way for Rust mods that share a crate.

## Declaring a call

The server declares the input with
[`#[call]`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/services/calls.rs)
and the answer's type with `output`. Listed in the manifest's `calls`, the
schema goes into the server's DLL:

```rust
/// What the trading post pays for an item.
#[enshwick::call(name = "you.trading-post:price-of", output = u32, version = 1)]
#[derive(Clone, Copy, Debug)]
pub struct PriceOf {
    /// The item.
    pub item: u32,
    /// How many the seller sells.
    pub count: u32,
}

fn manifest() -> &'static enshwick::abi::Manifest {
    enshwick::manifest!(id: "you.trading-post", side: SERVER, calls: [PriceOf])
}
```

- Name it after the serving mod, `<id>:<call>`. The loader refuses other names.
- `version` is the call's major, 1 when left out. A changed field or answer is a
  new major, which the server may serve beside the old one.
- Every type in the input and the answer has a [`Shape`](services.md#shapes)
  that a schema describes: numbers, `bool`, `Entity`, `Guid`, and the mod's own
  structs, enums and value types.
- The input and the answer have no padding, since another language reads them as
  bytes. The build fails on a gap: add its bytes as fields of their own, such as
  a `u8` and a `u16` between a `bool` and the next `u32`.

## Serving

[`load.serve_call(handler)`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/services/calls.rs)
serves the call during `init`. The handler runs on the caller's thread, so it
takes only what is safe from any thread:

```rust
# #[enshwick::call(name = "you.trading-post:price-of", output = u32)]
# #[derive(Clone, Copy, Debug)]
# pub struct PriceOf {
#     pub item: u32,
#     pub count: u32,
# }
fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    load.serve_call(|input: &PriceOf| {
        let coins = input.item * 10 * input.count;
        if input.count >= 10 { coins - coins / 10 } else { coins }
    })
}
```

A panic in the handler fails that call and turns the handler off until the next
start, with a warning in the log. Later calls answer that the server is off, and
the rest of the mod stays on. The status summary lists each served call with the
mods that call it, and the profile counts the handler's time against the server.

## Calling

A caller compiles the server's declaration. `cargo enshwick call` writes it from
the schema in the server's DLL without running it:

```bash
cargo enshwick call you.trading-post:price-of --from <game folder> --out src/price_of.rs
```

`--from` is the server's DLL, a folder with it in it or in its `mods/`, or a
schema file the server published. The file ends with checks that hold it to the
server's layout.
[`load.caller::<C>()`](https://github.com/enshwick/enshwick/blob/main/crates/enshwick/src/services/calls.rs)
then gives a `Caller`, which is `Copy` and runs the call from any thread:

```rust
# #[enshwick::call(name = "you.trading-post:price-of", output = u32)]
# #[derive(Clone, Copy, Debug)]
# pub struct PriceOf {
#     pub item: u32,
#     pub count: u32,
# }
use enshwick::services::CallError;

fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    let price_of = load.caller::<PriceOf>()?;
    load.on::<enshwick::events::WorldLoaded>(move |_| match price_of.call(&PriceOf { item: 7, count: 1 }) {
        Ok(coins) => enshwick::info!("item 7 sells for {coins}"),
        Err(CallError::Unmatched) => enshwick::info!("no trading post here"),
        Err(other) => enshwick::warn!("the trading post: {other}"),
    })
}
```

The loader matches the caller with the server once every mod is initialised, by
name and major, and the input and the answer must agree field by field and in
size. Without a match it logs the reason, such as a server that serves another
major, and every call answers `CallError::Unmatched`. A call to a server
switched off later answers `CallError::Off`, and a failed handler
`CallError::Failed`. Declare the server with `recommends`, or `requires` when
the mod is pointless without it.

### The schema

A call's schema sits in the server's manifest beside its
[points'](extending.md#the-schema), in the same format, as a `[[call]]` table:
`name`, `major`, `input`, `output`, `doc`, `input_size`, `output_size`, `layout`
(each input field's layout hash, as decimal text) and `output_layout`, with the
`[[type]]` tables of every type the input and the answer reach.

## Described calls

A runtime that knows a call only from its schema, such as a script engine, uses
`DescribedCall` with `Load::described_caller` and `Load::serve_described_call`.
The loader matches them the same way, and the input and the answer travel as
bytes. Both are `unsafe`, since the other side reads those bytes as its own
types. Every value a described caller sends, or a described server answers, is a
valid value of the type its layout hash names.

## Testing

The [fake loader](testing.md) serves another mod's call with
`loader.serve_call(handler)`, and calls the mod's own with
`loader.call(&input)`:

```rust
# #[enshwick::call(name = "you.trading-post:price-of", output = u32)]
# #[derive(Clone, Copy, Debug)]
# pub struct PriceOf {
#     pub item: u32,
# }
let loader = enshwick::test::FakeHost::new().install();
loader.serve_call(|input: &PriceOf| input.item * 10);
assert_eq!(loader.call(&PriceOf { item: 1 }), Err(enshwick::services::CallError::Unmatched));
```
