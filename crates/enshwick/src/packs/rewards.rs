//! A pack's `rewards.toml`: rewards for players' actions, read and checked.

use serde::Deserialize;
use toml::Spanned;

use crate::content::Guid;

use super::REWARDS_FILE;
use super::manifest::PackSetting;
use super::reader::Reader;
use super::values::{Number, NumberEntry, number};

/// `rewards.toml`.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RewardsFile {
    #[serde(default)]
    pub(super) reward: Vec<Spanned<RewardEntry>>,
}

/// One reward.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct RewardEntry {
    /// Its name in the pack, 1 to 64 of `[a-z0-9_-]`, which the world's
    /// data keeps who got it under.
    pub(super) id: String,
    /// The action it follows: one key of the table.
    pub(super) on: OnEntry,
    /// What it gives, in order.
    pub(super) give: Vec<GiveEntry>,
    /// How often one player gets it; every time when left out.
    pub(super) limit: Option<LimitEntry>,
    /// How likely, from 0 to 1, or `{ setting = "<key>" }`; always when
    /// left out.
    pub(super) chance: Option<NumberEntry<f32>>,
}

/// The action a reward follows; exactly one key.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct OnEntry {
    /// A player crafts this recipe.
    pub(super) craft: Option<String>,
    /// A player reaches this level; 0 for every level.
    pub(super) level_up: Option<u32>,
    /// A player joins the world.
    pub(super) join: Option<bool>,
    /// A player loots a container.
    pub(super) loot: Option<bool>,
    /// A player completes a quest.
    pub(super) quest: Option<bool>,
    /// A player eats, drinks or uses this item.
    pub(super) consume: Option<String>,
    /// A player picks this item up.
    pub(super) pickup: Option<String>,
}

/// One thing a reward gives.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema), serde(untagged))]
pub(super) enum GiveEntry {
    /// Items.
    Items(ItemsEntry),
    /// Experience.
    Experience(ExperienceEntry),
    /// A knowledge entry.
    Knowledge(KnowledgeEntry),
    /// A buff.
    Buff(BuffEntry),
}

/// Every field a gift takes, for a gift that names none of its kinds.
const GIVE_FIELDS: &[&str] = &["item", "count", "experience", "knowledge", "buff"];

impl<'de> Deserialize<'de> for GiveEntry {
    /// Told apart by the field that names the kind, so a misspelt field of
    /// that kind is named.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        use super::values::shape;
        use serde::de::Error as _;
        let table = toml::Table::deserialize(deserializer)?;
        let kind = ["item", "experience", "knowledge", "buff"]
            .into_iter()
            .find(|key| table.contains_key(*key));
        let value = toml::Value::Table(table);
        match kind {
            Some("item") => shape(value).map(Self::Items),
            Some("experience") => shape(value).map(Self::Experience),
            Some("knowledge") => shape(value).map(Self::Knowledge),
            Some(_) => shape(value).map(Self::Buff),
            None => Err(
                match value
                    .as_table()
                    .and_then(|t| t.keys().find(|key| !GIVE_FIELDS.contains(&key.as_str())))
                {
                    Some(field) => D::Error::unknown_field(field, GIVE_FIELDS),
                    None => {
                        D::Error::custom("a gift needs `item`, `experience`, `knowledge` or `buff`")
                    }
                },
            ),
        }
    }
}

/// `count` of `item`, all or nothing.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct ItemsEntry {
    /// The item: the pack's own name, a content id or a vanilla item.
    pub(super) item: String,
    /// How many, or `{ setting = "<key>" }`; 1 when left out.
    pub(super) count: Option<NumberEntry<u32>>,
}

/// Experience, added to the player's next gains.
#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct ExperienceEntry {
    /// How much, or `{ setting = "<key>" }`.
    pub(super) experience: NumberEntry<u32>,
}

/// A knowledge entry, set.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct KnowledgeEntry {
    /// The entry's GUID.
    pub(super) knowledge: String,
}

/// A buff, for its type's own duration.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub(super) struct BuffEntry {
    /// The buff type's GUID.
    pub(super) buff: String,
}

/// How often one player gets a reward.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(rename_all = "lowercase")]
pub(super) enum LimitEntry {
    /// Every time.
    #[default]
    Every,
    /// Once per player in the world.
    Once,
    /// Once per player each day.
    Daily,
    /// Once in the world.
    World,
}

/// A pack's reward, checked, its references resolved.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PackReward {
    pub(super) name: String,
    pub(super) on: PackOn,
    pub(super) give: Vec<PackGive>,
    pub(super) limit: LimitEntry,
    pub(super) chance: Option<Number<f32>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PackOn {
    Craft(Guid),
    LevelUp(u32),
    Join,
    Loot,
    Quest,
    Consume(Guid),
    Pickup(Guid),
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum PackGive {
    Items(Guid, Number<u32>),
    Experience(Number<u32>),
    Knowledge(Guid),
    Buff(Guid),
}

impl PackReward {
    /// Whether a number of it follows a setting.
    #[cfg_attr(
        not(feature = "unstable"),
        expect(dead_code, reason = "rewards are given only with `unstable`")
    )]
    pub(super) fn follows_settings(&self) -> bool {
        self.chance.as_ref().is_some_and(Number::follows)
            || self.give.iter().any(|give| match give {
                PackGive::Items(_, count) => count.follows(),
                PackGive::Experience(amount) => amount.follows(),
                PackGive::Knowledge(_) | PackGive::Buff(_) => false,
            })
    }
}

/// The checked rewards of `file`, whose text is `text`, their settings
/// among `settings`.
pub(super) fn rewards(
    reader: &mut Reader<'_>,
    file: &RewardsFile,
    text: &str,
    settings: &[PackSetting],
) -> Vec<PackReward> {
    let mut out: Vec<PackReward> = Vec::new();
    for entry in &file.reward {
        let at = entry.span().start;
        let e = entry.get_ref();
        let error = |reader: &mut Reader<'_>, why: String| {
            reader.error(REWARDS_FILE, text, at, format!("reward {}: {why}", e.id));
        };
        let well_formed = (1..=64).contains(&e.id.len())
            && e.id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_');
        if !well_formed {
            error(reader, "a name is 1 to 64 of [a-z0-9_-]".to_owned());
        } else if out.iter().any(|r| r.name == e.id) {
            error(reader, "defined twice".to_owned());
        }
        let chance = e
            .chance
            .as_ref()
            .map(|c| number(reader, REWARDS_FILE, text, at, c, settings));
        if chance
            .as_ref()
            .and_then(|c| c.as_ref())
            .is_some_and(|c| !(0.0..=1.0).contains(&c.default()))
        {
            error(reader, "a chance is from 0 to 1".to_owned());
        }
        if e.give.is_empty() {
            error(reader, "it gives nothing".to_owned());
        }
        let on = on(reader, text, at, &e.on);
        if on.is_none() {
            error(
                reader,
                "`on` names one of craft, level_up, join, loot, quest, consume or pickup"
                    .to_owned(),
            );
        }
        let give: Vec<PackGive> = e
            .give
            .iter()
            .filter_map(|give| match give {
                GiveEntry::Items(items) => {
                    let (guid, _) =
                        reader.reference(REWARDS_FILE, text, at, &items.item, "item", "")?;
                    let count = match &items.count {
                        Some(count) => number(reader, REWARDS_FILE, text, at, count, settings)?,
                        None => Number::Fixed(1),
                    };
                    Some(PackGive::Items(guid, count))
                }
                GiveEntry::Experience(experience) => number(
                    reader,
                    REWARDS_FILE,
                    text,
                    at,
                    &experience.experience,
                    settings,
                )
                .map(PackGive::Experience),
                GiveEntry::Knowledge(knowledge) => {
                    guid(reader, text, at, &knowledge.knowledge).map(PackGive::Knowledge)
                }
                GiveEntry::Buff(buff) => guid(reader, text, at, &buff.buff).map(PackGive::Buff),
            })
            .collect();
        if let Some(on) = on
            && give.len() == e.give.len()
            && chance.as_ref().is_none_or(Option::is_some)
        {
            out.push(PackReward {
                name: e.id.clone(),
                on,
                give,
                limit: e.limit.unwrap_or_default(),
                chance: chance.flatten(),
            });
        }
    }
    out
}

/// The action `on` names, `None` with no key or more than one.
fn on(reader: &mut Reader<'_>, text: &str, at: usize, on: &OnEntry) -> Option<PackOn> {
    let flags = [
        (on.join, PackOn::Join),
        (on.loot, PackOn::Loot),
        (on.quest, PackOn::Quest),
    ];
    let keys = usize::from(on.craft.is_some())
        + usize::from(on.level_up.is_some())
        + usize::from(on.consume.is_some())
        + usize::from(on.pickup.is_some())
        + flags.iter().filter(|(set, _)| set.is_some()).count();
    if keys != 1 {
        return None;
    }
    let mut item =
        |name: &str, kind: &'static str| reader.reference(REWARDS_FILE, text, at, name, kind, "");
    if let Some(recipe) = &on.craft {
        return item(recipe, "recipe").map(|(guid, _)| PackOn::Craft(guid));
    }
    if let Some(consumed) = &on.consume {
        return item(consumed, "item").map(|(guid, _)| PackOn::Consume(guid));
    }
    if let Some(picked) = &on.pickup {
        return item(picked, "item").map(|(guid, _)| PackOn::Pickup(guid));
    }
    if let Some(level) = on.level_up {
        return Some(PackOn::LevelUp(level));
    }
    flags
        .into_iter()
        .find_map(|(set, action)| (set == Some(true)).then_some(action))
}

/// The GUID `text` spells, with an error at `at` when it is none.
fn guid(reader: &mut Reader<'_>, file_text: &str, at: usize, text: &str) -> Option<Guid> {
    let guid = Guid::try_parse(text);
    if guid.is_none() {
        reader.error(REWARDS_FILE, file_text, at, format!("{text:?} is no GUID"));
    }
    guid
}

#[cfg(feature = "unstable")]
impl PackReward {
    /// The reward to declare with [`Load::add_reward`](crate::Load::add_reward),
    /// its numbers from the world's settings `values`; a chance outside 0
    /// to 1 is held to that range.
    pub(super) fn reward(&self, values: &toml::Table) -> crate::rewards::Reward {
        use crate::abi::{BuffTypeId, KnowledgeId, RecipeId};
        use crate::rewards::{Action, Give, Limit, Reward};

        let on = match self.on {
            PackOn::Craft(recipe) => Action::Craft(RecipeId::from_guid(recipe)),
            PackOn::LevelUp(level) => Action::LevelUp(level),
            PackOn::Join => Action::Join,
            PackOn::Loot => Action::Loot,
            PackOn::Quest => Action::Quest,
            PackOn::Consume(item) => Action::Consume(crate::ItemId::from_guid(item)),
            PackOn::Pickup(item) => Action::Pickup(crate::ItemId::from_guid(item)),
        };
        let limit = match self.limit {
            LimitEntry::Every => Limit::Every,
            LimitEntry::Once => Limit::Once,
            LimitEntry::Daily => Limit::Daily,
            LimitEntry::World => Limit::World,
        };
        let mut reward = Reward::new(&self.name, on).with_limit(limit);
        reward.chance = self.chance.as_ref().map(|c| c.get(values).clamp(0.0, 1.0));
        for give in &self.give {
            reward = reward.with(match give {
                PackGive::Items(item, count) => Give::Items {
                    item: crate::ItemId::from_guid(*item),
                    count: count.get(values),
                },
                PackGive::Experience(amount) => Give::Experience(amount.get(values)),
                PackGive::Knowledge(id) => Give::Knowledge(KnowledgeId::from_guid(*id)),
                PackGive::Buff(id) => Give::Buff(BuffTypeId::from_guid(*id)),
            });
        }
        reward
    }
}
