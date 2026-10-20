# enshwick-loader-api

The modding API of Enshwick Loader for Enshrouded, and the one crate a mod
depends on. A mod is a DLL that implements `enshwick_loader_api::Mod`, links
its hooks while the loader initialises it, and runs inside the game or the
dedicated server once the loader arms them.

```rust
struct Hello;

impl enshwick_loader_api::Mod for Hello {
    fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
        enshwick_loader_api::manifest!(id: "hello", side: SERVER)
    }

    fn init(_load: &mut enshwick_loader_api::Load<'_>) -> Result<Self, enshwick_loader_api::Off> {
        enshwick_loader_api::info!("hello from the server");
        Ok(Self)
    }
}

enshwick_loader_api::declare_mod!(Hello);
```

With the `test` feature, `enshwick_loader_api::test` is a fake loader for a
mod's unit tests; turn it on under `[dev-dependencies]` only. The macros come
from `enshwick-loader-macros`, an internal crate this one re-exports. See
[the modding book](../../book/src/SUMMARY.md).

Licensed under [MIT](../../LICENSE).
