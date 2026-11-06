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
    Concepts => "concepts.md",
    LoaderAndAbi => "concepts/loader-and-abi.md",
    Lifecycle => "concepts/lifecycle.md",
    SidesAndHosting => "concepts/sides-and-hosting.md",
    JoinCheck => "concepts/join-check.md",
    Settings => "settings.md",
    Logging => "logging.md",
    Events => "events.md",
    Altars => "altars.md",
    Progress => "progress.md",
    Services => "services.md",
    Luau => "luau.md",
    ReachingIntoTheGame => "reaching-into-the-game.md",
    FindingTheGame => "finding-the-game.md",
    TypedStability => "concepts/typed-stability.md",
    ResourceValues => "resource-values.md",
    Hooks => "hooks.md",
    MidFunctionHooks => "mid-function-hooks.md",
    CodePatches => "code-patches.md",
    Pacing => "pacing.md",
    GameFrame => "game-frame.md",
    Ecs => "ecs.md",
    Content => "content.md",
    Items => "content/items.md",
    DisabledRecipes => "content/disabled-recipes.md",
    RandomLoot => "content/random-loot.md",
    Resources => "content/resources.md",
    Texts => "content/texts.md",
    ModWithoutRust => "content/a-mod-without-rust.md",
    Packs => "content/packs.md",
    Limits => "content/limits.md",
    Inventories => "inventories.md",
    Releasing => "releasing.md",
    Stability => "stability.md",
    AbiReference => "abi-reference.md",
    ModsInC => "mods-in-c.md",
    Faq => "faq.md",
}

#[cfg(feature = "test")]
#[doc = include_str!("../../../book/src/testing.md")]
#[expect(dead_code, reason = "the doc comment carries the chapter")]
struct Testing;

#[cfg(all(feature = "test", feature = "unstable"))]
#[doc = include_str!("../../../book/src/calls.md")]
#[expect(dead_code, reason = "the doc comment carries the chapter")]
struct Calls;

#[cfg(feature = "test")]
#[doc = include_str!("../../../book/src/extending.md")]
#[expect(dead_code, reason = "the doc comment carries the chapter")]
struct Extending;

// The typed API's chapter runs only with its feature.
#[cfg(feature = "typed")]
mod typed {
    chapters! {
        GameTypes => "game-types.md",
        WeatherPhenomena => "weather-phenomena.md",
        Templates => "content/templates.md",
        Props => "content/props.md",
        Enemies => "content/enemies.md",
        Spawns => "content/spawns.md",
    }
}

// Chapters whose examples use unstable calls, if only to show a stable one
// in context, run only with the `unstable` feature.
#[cfg(feature = "unstable")]
mod unstable {
    chapters! {
        GettingStarted => "getting-started.md",
        FirstMod => "first-mod.md",
        Threads => "concepts/threads.md",
        GameEvents => "game-events.md",
        World => "world.md",
        Effects => "effects.md",
        Daytime => "daytime.md",
        GameSettings => "game-settings.md",
        Buffs => "buffs.md",
        Knowledge => "knowledge.md",
        Weather => "weather.md",
        Terrain => "terrain.md",
        Map => "map.md",
        ItemAndEntityData => "item-and-entity-data.md",
        Survivors => "survivors.md",
        Vetoes => "vetoes.md",
        ItemActions => "content/item-actions.md",
        Rewards => "content/rewards.md",
        Actions => "actions.md",
        Controls => "controls.md",
        Input => "input.md",
        Chat => "chat.md",
        Messages => "messages.md",
        Interface => "interface.md",
        Loot => "content/loot.md",
        Stats => "stats.md",
        Affixes => "affixes.md",
        Recipes => "content/recipes.md",
        Workshops => "content/workshops.md",
        Damage => "damage.md",
        Rates => "rates.md",
        State => "state.md",
    }
}
