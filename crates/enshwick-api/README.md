# enshwick

The modding API for Enshrouded. A mod is a DLL that implements
`enshwick_api::Mod`, links its hooks while the loader initialises it, and runs
inside the game or the dedicated server once the loader arms them.

```rust
struct Hello;

impl enshwick_api::Mod for Hello {
    fn manifest() -> &'static enshwick_api::abi::Manifest {
        enshwick_api::manifest!(id: "hello", side: SERVER)
    }

    fn init(_load: &mut enshwick_api::Load<'_>) -> Result<Self, enshwick_api::Off> {
        enshwick_api::info!("hello from the server");
        Ok(Self)
    }
}

enshwick_api::declare_mod!(Hello);
```

Licensed under [MIT](../../LICENSE).
