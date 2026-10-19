# enshwick

The modding API for Enshrouded. A mod is a DLL that implements `enshwick::Mod`,
links its hooks while the loader initialises it, and runs inside the game or the
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

Licensed under [MIT](../../LICENSE).
