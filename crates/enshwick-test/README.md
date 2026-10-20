# enshwick-test

A fake Enshwick loader for a mod's unit tests. `FakeHost` answers the mod's
calls the way you configure it, records what the mod asks for (log lines,
faults, hooks, served resources, texts, subscriptions), raises events and calls
a linked detour as the game would.

```toml
[dev-dependencies]
enshwick-test = { git = "https://github.com/themaiby/enshwick" }
```

```rust
use enshwick_api::abi::Status;

#[test]
fn starts() {
    let loader = enshwick_test::FakeHost::new()
        .symbol("building.max_altars", 0x1_4012_3450)
        .settings("altars = 16\n")
        .install();
    // SAFETY: the exports of `declare_mod!` in this crate.
    let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
    assert_eq!(status, Status::OK);
    assert_eq!(loader.hooks()[0].target, 0x1_4012_3450);
}
```

A mod initialises once per process, as in the game: write one test per mod
life and run them with `cargo nextest`, which gives each test its own process.
Under `cargo test`, tests in one binary share it; the fake then serves one test
at a time.

Licensed under [MIT](../../LICENSE).
