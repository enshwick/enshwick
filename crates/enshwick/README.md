# enshwick

The modding API of Enshwick Loader for Enshrouded, and the one crate a mod
depends on. A mod is a DLL that implements `enshwick::Mod`, links
its hooks while the loader initialises it, and runs inside the game or the
dedicated server once the loader arms them.

```rust
struct Hello;

impl enshwick::Mod for Hello {
    fn manifest() -> &'static enshwick::abi::Manifest {
        enshwick::manifest!(id: "hello", side: SERVER)
    }

    fn init(_load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
        enshwick::info!("hello from the server");
        Ok(Self)
    }
}

enshwick::declare_mod!(Hello);
```

With the `test` feature, `enshwick::test` is a fake loader for a
mod's unit tests; turn it on under `[dev-dependencies]` only. The macros come
from `enshwick-macros`, an internal crate this one re-exports. See
[the modding book](https://github.com/enshwick/enshwick/blob/main/book/src/SUMMARY.md).

Licensed under [MIT](https://github.com/enshwick/enshwick/blob/main/LICENSE).
