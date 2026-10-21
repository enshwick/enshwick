//! Affixes and stats by hand: two affixes of the probe's own, an item with
//! an entity found in a container and rolled from a seed, its lines read
//! and its old stats put back; and the `/probe stat`, `/probe stats` and
//! `/probe affix` commands an administrator tries them with in game.

use enshwick_loader_api::affixes::{AffixDef, Affixes, Categories, Range, Rarity, RollRules};
use enshwick_loader_api::inventory::{Category, Inventories};
use enshwick_loader_api::item_data::ItemInstance;
use enshwick_loader_api::stats::{Lasting, Modifier, Op, Stat, StatDef, Stats, builtin};
use enshwick_loader_api::world::Player;
use enshwick_loader_api::world::{ComponentType, World};
use enshwick_loader_api::{Load, Off};

/// The probe's key for the modifiers its commands set.
const KEY: &str = "probe-command";
/// The seed of the probe's check roll.
const SEED: u64 = 0x5eed;

/// The affix checks and commands.
#[derive(Debug, Clone)]
pub(crate) struct AffixChecks {
    affixes: Affixes,
    stats: Stats,
    inventories: Inventories,
    world: World,
    setup: ComponentType,
    /// Every stat the commands may set, by id.
    kinds: Vec<(String, Stat)>,
}

impl AffixChecks {
    pub(crate) fn new(load: &mut Load<'_>) -> Result<Self, Off> {
        let world = load.world()?;
        let mut kinds = Vec::new();
        for (id, _) in builtin::ALL {
            kinds.push((id.to_owned(), load.open_stat(id)?));
        }
        let lucky = load.stat("lucky", &StatDef::additive("probe.lucky").range(0.0, 100.0))?;
        kinds.push(("probe:lucky".to_owned(), lucky));
        let health = load.open_stat(builtin::HEALTH_MAX)?;
        load.affix(
            "sturdy",
            &AffixDef::new(
                "probe.affix.sturdy",
                Categories::EQUIPMENT | Categories::WEAPONS,
            )
            .with(Range::new(health, Op::Add, 10.0, 30.0).step(1.0)),
        )?;
        load.affix(
            "fortunate",
            &AffixDef::new(
                "probe.affix.fortunate",
                Categories::EQUIPMENT | Categories::WEAPONS,
            )
            .with(Range::new(lucky, Op::Add, 1.0, 5.0).step(1.0))
            .rarity(Rarity::Rare)
            .weight(30),
        )?;
        Ok(Self {
            affixes: load.affixes(),
            stats: load.stats()?,
            inventories: load.inventories()?,
            setup: world.component_type(load, "keen::ecs::InventorySetup")?,
            world,
            kinds,
        })
    }

    /// The checks, added to `extras`, or `None` with the reason logged.
    pub(crate) fn register(
        load: &mut Load<'_>,
        extras: &mut Vec<crate::Check>,
    ) -> Option<std::sync::Arc<Self>> {
        match Self::new(load) {
            Ok(checks) => {
                let checks = std::sync::Arc::new(checks);
                let run = std::sync::Arc::clone(&checks);
                extras.push(Box::new(move || run.run()));
                Some(checks)
            }
            Err(off) => {
                enshwick_loader_api::warn!("FAIL affixes: {off}");
                None
            }
        }
    }

    pub(crate) fn run(&self) {
        let reader = self.inventories.read();
        let found = self
            .world
            .entities(self.setup)
            .iter()
            .flat_map(|placed| reader.slots(placed.entity))
            .filter_map(|slot| ItemInstance::of(&slot))
            .find(|item| {
                self.inventories.item_info(item.item).is_some_and(|info| {
                    Categories(Categories::EQUIPMENT.0 | Categories::WEAPONS.0).holds(info.category)
                })
            });
        let Some(item) = found else {
            enshwick_loader_api::info!("SKIP affixes: no weapon or armour in any container");
            return;
        };
        let before = self.stats.item(item);
        match self
            .affixes
            .roll(item, RollRules::new(2, 2), Some(SEED), false)
        {
            Ok(2) => enshwick_loader_api::info!("PASS affixes: {item:?} rolled 2"),
            other => {
                enshwick_loader_api::warn!("FAIL affixes: roll of {item:?}: {other:?}");
                return;
            }
        }
        let first = self.stats.item(item);
        let _again = self
            .affixes
            .roll(item, RollRules::new(2, 2), Some(SEED), false);
        if self.stats.item(item) == first {
            enshwick_loader_api::info!("PASS affixes: the same seed rolls {:?}", first.affixes);
        } else {
            enshwick_loader_api::warn!("FAIL affixes: the same seed rolled otherwise");
        }
        let lines = self.affixes.lines(item);
        let named: Vec<String> = lines
            .iter()
            .filter_map(|l| {
                self.affixes
                    .info(l.affix?)
                    .map(|i| format!("{} {:?} {}", i.text, l.op, l.value))
            })
            .collect();
        if named.len() == 2 {
            enshwick_loader_api::info!("PASS affixes: lines {named:?}");
        } else {
            enshwick_loader_api::warn!("FAIL affixes: lines {lines:?}");
        }
        match self.stats.set_item(item, &before) {
            Ok(()) => enshwick_loader_api::info!("PASS affixes: {item:?} put back"),
            Err(why) => enshwick_loader_api::warn!("FAIL affixes: putting back {item:?}: {why}"),
        }
    }

    /// `/probe stat <id> <value|off>`: an `Add` modifier on the caller.
    pub(crate) fn stat(&self, player: &Player, id: &str, value: &str) -> String {
        let Some(&(_, stat)) = self.kinds.iter().find(|(known, _)| known == id) else {
            let ids: Vec<&str> = self.kinds.iter().map(|(id, _)| id.as_str()).collect();
            return format!("stats: {}", ids.join(", "));
        };
        let done = if value == "off" {
            self.stats.remove(player.entity, KEY, stat)
        } else {
            let Ok(value) = value.parse::<f32>() else {
                return "/probe stat <id> <value|off>".to_owned();
            };
            let modifier = Modifier::new(stat, Op::Add, value);
            self.stats
                .set(player.entity, KEY, modifier, Lasting::UntilRemoved)
        };
        match done {
            Ok(()) => format!("{id}: {value}; the value follows after the next step"),
            Err(why) => format!("{id}: {why}"),
        }
    }

    /// `/probe stats`: the caller's values that differ from the defaults and
    /// where they come from.
    pub(crate) fn stats_of(&self, player: &Player) -> String {
        let mut lines: Vec<String> = self
            .kinds
            .iter()
            .filter_map(|(id, stat)| {
                let value = self.stats.value(player.entity, *stat);
                let default = self.stats.info(*stat)?.def.default;
                ((value - default).abs() > f32::EPSILON).then(|| format!("{id} = {value}"))
            })
            .collect();
        lines.extend(
            self.stats
                .sources(player.entity, None)
                .iter()
                .map(|s| format!("  {:?}: stat {} {:?} {}", s.origin, s.stat, s.op, s.value)),
        );
        if lines.is_empty() {
            "no stats differ from the defaults".to_owned()
        } else {
            lines.join("\n")
        }
    }

    /// `/probe affix`: rolls every item the caller wears anew, one to three
    /// affixes each.
    pub(crate) fn reroll(&self, player: &Player) -> String {
        let reader = self.inventories.read();
        let worn: Vec<ItemInstance> = reader
            .slots(player.entity)
            .iter()
            .filter(|slot| slot.category == Category::EQUIPMENT)
            .filter_map(ItemInstance::of)
            .collect();
        let rolled: Vec<String> = worn
            .iter()
            .map(
                |&item| match self.affixes.roll(item, RollRules::new(1, 3), None, false) {
                    Ok(count) => format!("item {:#x}: {count} affixes", item.item),
                    Err(why) => format!("item {:#x}: {why}", item.item),
                },
            )
            .collect();
        if rolled.is_empty() {
            "you wear nothing that rolls".to_owned()
        } else {
            rolled.join("\n")
        }
    }
}

/// Answers `/probe stat`, `/probe stats` and `/probe affix`.
pub(crate) fn command(
    checks: Option<&AffixChecks>,
    player: &Player,
    word: &str,
    rest: &[&str],
) -> String {
    let Some(checks) = checks else {
        return "stats and affixes are off".to_owned();
    };
    match (word, rest) {
        ("stat", [id, value]) => checks.stat(player, id, value),
        ("stats", []) => checks.stats_of(player),
        ("affix", []) => checks.reroll(player),
        _ => "/probe stat <id> <value|off>|stats|affix".to_owned(),
    }
}
