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
    GameEvents => "game-events.md",
    World => "world.md",
    Effects => "effects.md",
    Daytime => "daytime.md",
    GameSettings => "game-settings.md",
    Damage => "damage.md",
    Buffs => "buffs.md",
    Knowledge => "knowledge.md",
    Weather => "weather.md",
    Terrain => "terrain.md",
    Altars => "altars.md",
    Survivors => "survivors.md",
    Interactions => "interactions.md",
    Vetoes => "vetoes.md",
    Progress => "progress.md",
    Actions => "actions.md",
    Commands => "commands.md",
    Messages => "messages.md",
    Interface => "interface.md",
    State => "state.md",
    Services => "services.md",
    FindingTheGame => "finding-the-game.md",
    Hooks => "hooks.md",
    MidFunctionHooks => "mid-function-hooks.md",
    CodePatches => "code-patches.md",
    Pacing => "pacing.md",
    GameFrame => "game-frame.md",
    Ecs => "ecs.md",
    Content => "content.md",
    Items => "content/items.md",
    Recipes => "content/recipes.md",
    DisabledRecipes => "content/disabled-recipes.md",
    Workshops => "content/workshops.md",
    Loot => "content/loot.md",
    RandomLoot => "content/random-loot.md",
    Resources => "content/resources.md",
    Texts => "content/texts.md",
    Packs => "content/packs.md",
    Inventories => "inventories.md",
    ItemData => "item-data.md",
    EntityData => "entity-data.md",
    Stats => "stats.md",
    Affixes => "affixes.md",
    Releasing => "releasing.md",
    Servers => "servers.md",
    AbiReference => "abi-reference.md",
    Faq => "faq.md",
}

#[cfg(feature = "test")]
#[doc = include_str!("../../../book/src/testing.md")]
#[expect(dead_code, reason = "the doc comment carries the chapter")]
struct Testing;

#[cfg(feature = "test")]
#[doc = include_str!("../../../book/src/extending.md")]
#[expect(dead_code, reason = "the doc comment carries the chapter")]
struct Extending;

#[cfg(feature = "test")]
#[doc = include_str!("../../../book/src/migrations.md")]
#[expect(dead_code, reason = "the doc comment carries the chapter")]
struct Migrations;
