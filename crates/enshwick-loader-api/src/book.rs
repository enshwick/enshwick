//! The chapters of the modding book in `book/src`, whose Rust examples run as
//! doctests of this crate. Only `cargo test --doc` reads this module.

macro_rules! chapters {
    ($($name:ident => $path:literal,)*) => {
        $(
            #[doc = include_str!(concat!("../../../book/src/", $path))]
            #[expect(dead_code, reason = "the doc comment carries the chapter")]
            struct $name;
        )*
    };
}

chapters! {
    Introduction => "introduction.md",
    GettingStarted => "getting-started.md",
    Concepts => "concepts.md",
    LoaderAndAbi => "concepts/loader-and-abi.md",
    Lifecycle => "concepts/lifecycle.md",
    SidesAndHosting => "concepts/sides-and-hosting.md",
    JoinCheck => "concepts/join-check.md",
    Threads => "concepts/threads.md",
    Settings => "settings.md",
    Logging => "logging.md",
    Events => "events.md",
    World => "world.md",
    Commands => "commands.md",
    State => "state.md",
    Services => "services.md",
    FindingTheGame => "finding-the-game.md",
    Hooks => "hooks.md",
    MidFunctionHooks => "mid-function-hooks.md",
    CodePatches => "code-patches.md",
    GameFrame => "game-frame.md",
    Ecs => "ecs.md",
    Content => "content.md",
    Items => "content/items.md",
    Recipes => "content/recipes.md",
    Workshops => "content/workshops.md",
    Loot => "content/loot.md",
    RandomLoot => "content/random-loot.md",
    Resources => "content/resources.md",
    Texts => "content/texts.md",
    Inventories => "inventories.md",
    Releasing => "releasing.md",
    AbiReference => "abi-reference.md",
    Faq => "faq.md",
}
