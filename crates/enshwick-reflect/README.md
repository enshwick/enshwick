# enshwick-reflect

Reads the reflection data and ECS system records of Keen's Holistic engine from
an Enshrouded server or client image, so the Enshwick loader finds types, fields
and systems by name instead of by address.

```rust,no_run
use enshwick_reflect::{Image, Layout, Reflection};

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let bytes = std::fs::read("enshrouded_server.exe")?;
let image = Image::parse(&bytes, Layout::File, None)?;
let reflection = Reflection::scan(&image);
let system = reflection.system("add_buildzones_server")?;
assert_ne!(system.entry(), 0);
# Ok(())
# }
```

Licensed under [MIT](../../LICENSE).
