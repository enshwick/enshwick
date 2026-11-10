//! Data packs: items, recipes, workshops, loot and texts described in
//! TOML files instead of code, and changes of vanilla values.
//!
//! A pack is a set of files: `items.toml`, `recipes.toml`, `workshops.toml`,
//! `loot.toml` and `texts.toml`, each optional, and the PNG icons the items
//! name. [`Pack::parse`] reads and checks them against the types below and
//! reports every problem with its file, line and column; [`Load::add_pack`]
//! adds what they describe through the same calls code uses, beside vanilla
//! content, never in its place. A mod embeds a pack with
//! [`PackFiles::with`]; a folder `mods/<id>/` with a `pack.toml`
//! ([`PackManifest`]) and no DLL is a pack the loader runs on its own.
//!
//! Every name a pack defines lives in its namespace: `lantern` in the pack
//! `you.lanterns` is the content id `you.lanterns:lantern`. A reference is a name of
//! the pack, a full content id of another mod (`you.lockers:locker`), or a vanilla
//! GUID (`6943053a-32c1-4718-ae22-e61427ddf54d`).
//!
//! # Examples
//!
//! ```
//! # extern crate enshwick_core as enshwick;
//! use enshwick::packs::{Pack, PackFiles};
//!
//! let files = PackFiles::new()
//!     .with("items.toml", r#"
//! [[item]]
//! id = "lantern"
//! like = "6943053a-32c1-4718-ae22-e61427ddf54d"
//! name = "lantern.name"
//! "#)
//!     .with("texts.toml", "[en]\n\"lantern.name\" = \"Lantern\"\n");
//! let pack = Pack::parse("you.lanterns", &files)?;
//! assert_eq!(pack.ids(), ["you.lanterns:lantern"]);
//! # Ok::<(), enshwick::packs::ParsePackError>(())
//! ```

use crate::content::Guid;
use crate::content::icons::{Icon, Pixels};
use crate::content::items::{FuelSetup, ImpactUiValue, NewItem};
use crate::content::recipes::NewRecipe;
use crate::content::workshops::{NewWorkshop, WorkshopId};
use crate::locks::Relock as _;
use crate::{Load, Off};

/// The manifest file of a pack folder.
pub const MANIFEST_FILE: &str = "pack.toml";
/// The items file.
pub const ITEMS_FILE: &str = "items.toml";
/// The recipes file.
pub const RECIPES_FILE: &str = "recipes.toml";
/// The workshops file.
pub const WORKSHOPS_FILE: &str = "workshops.toml";
/// The loot file.
pub const LOOT_FILE: &str = "loot.toml";
/// The rewards file, rewards for players' actions.
pub const REWARDS_FILE: &str = "rewards.toml";
/// The texts file, in the form of a mod's [texts](crate::i18n).
pub const TEXTS_FILE: &str = "texts.toml";
/// The sounds file, the pack's own sounds from WAV files in its folder.
pub const SOUNDS_FILE: &str = "sounds.toml";
/// Every file a pack folder may hold besides its icons and sounds, in the
/// order the loader reads them.
pub const FILES: [&str; 8] = [
    MANIFEST_FILE,
    ITEMS_FILE,
    RECIPES_FILE,
    WORKSHOPS_FILE,
    LOOT_FILE,
    REWARDS_FILE,
    TEXTS_FILE,
    SOUNDS_FILE,
];
/// The extension of a script, a Luau file the loader runs for a pack that
/// has a [`SCRIPT_MAIN`].
pub const SCRIPT_EXTENSION: &str = "luau";
/// The script that makes a pack a Luau mod; the loader runs it at start.
pub const SCRIPT_MAIN: &str = "main.luau";
/// The end of a Luau definitions file's name, which editors read and the
/// loader leaves out.
pub const DEFINITIONS_SUFFIX: &str = ".d.luau";
/// The most scripts a pack may hold.
pub const MAX_SCRIPTS: usize = 256;
/// How many folders deep below the pack's folder the loader looks for
/// scripts; deeper folders are not read.
pub const MAX_SCRIPT_DEPTH: usize = 4;
/// The largest file a pack may hold: 4 MiB.
pub const MAX_FILE: u64 = 4 << 20;
/// The most icons a pack may hold under `icons/`.
pub const MAX_ICONS: usize = 1024;
/// The largest icon a pack may hold: 1 MiB.
pub const MAX_ICON_FILE: u64 = 1 << 20;

impl Load<'_> {
    /// Adds what `pack` describes, on the server and the client alike:
    /// its workshops, items, recipes, texts and loot, each through the call
    /// code uses, beside vanilla content, and declares its changes of
    /// vanilla values with [`Load::change`].
    ///
    /// # Errors
    ///
    /// [`Off`] as the calls it makes return it, such as for a content id
    /// the loader refuses, among them another mod's content id that mod
    /// does not export.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// const ITEMS: &str = r#"
    /// [[item]]
    /// id = "lantern"
    /// like = "6943053a-32c1-4718-ae22-e61427ddf54d"
    /// "#;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let files = PackFiles::new().with("items.toml", ITEMS);
    ///     load.add_pack(&Pack::parse("you.my-mod", &files)?)
    /// }
    /// ```
    pub fn add_pack(&mut self, pack: &Pack) -> Result<(), Off> {
        for name in &pack.named {
            crate::content::named(id(name)?)?;
        }
        let texts = self.texts_from(&pack.texts)?;
        let tag = |load: &mut Self, key: &Option<String>| -> Result<Option<Guid>, Off> {
            key.as_ref()
                .map(|key| {
                    let tag = id(&format!("{}:text/{key}", pack.namespace))?;
                    load.add_texts(&texts, &[(key, tag)])?;
                    Ok(tag.guid())
                })
                .transpose()
        };
        for workshop in &pack.workshops {
            let new = NewWorkshop {
                named: workshop.name.is_some(),
                described: workshop.description.is_some(),
                recipe_cost: workshop.cost,
                debug_name: workshop.debug_name.clone(),
                ..NewWorkshop::new(id(&workshop.id)?, workshop.like)
            };
            new.check()?;
            for (key, tag) in [
                (&workshop.name, new.name()),
                (&workshop.description, new.description()),
            ] {
                if let Some(key) = key {
                    self.add_texts(&texts, &[(key, tag)])?;
                }
            }
            self.add_workshop(new)?;
        }
        for item in &pack.items {
            let own = id(&item.id)?;
            let name = tag(self, &item.name)?;
            let description = tag(self, &item.description)?;
            let mut stats = Vec::new();
            for (label, value, format) in &item.stats {
                let label = tag(self, &Some(label.clone()))?.unwrap_or(Guid([0; 16]));
                stats.push(ImpactUiValue {
                    loca_id: label,
                    value: *value,
                    value_format: *format,
                });
            }
            let meter = item
                .meter
                .map(|(max, start)| -> Result<FuelSetup, Off> {
                    Ok(FuelSetup {
                        template: id(&format!("{}.meter", item.id))?,
                        max_fuel_charges: max,
                        start,
                    })
                })
                .transpose()?;
            let icon = item
                .icon
                .as_ref()
                .map(|icon| -> Result<Icon, Off> {
                    let (like, pixels) = match icon {
                        PackIcon::Tint { like, tint } => (*like, Pixels::Tint(*tint)),
                        PackIcon::Png(bytes) => {
                            let bytes: &'static [u8] = Box::leak(bytes.clone().into_boxed_slice());
                            (item.like, Pixels::Png(bytes))
                        }
                    };
                    Ok(Icon {
                        id: id(&format!("{}.icon", item.id))?,
                        like,
                        pixels,
                    })
                })
                .transpose()?;
            self.add_item(NewItem {
                name,
                description,
                debug_name: item.debug_name.clone(),
                meter,
                stats,
                icon,
                ..NewItem::new(own, item.like)
            })?;
        }
        self.add_pack_rest(pack)
    }

    /// Adds `pack`'s recipes, changes of values and loot.
    fn add_pack_rest(&mut self, pack: &Pack) -> Result<(), Off> {
        for recipe in &pack.recipes {
            self.push_recipe(NewRecipe {
                id: id(&recipe.id)?,
                like: recipe.like,
                output: recipe.output,
                output_count: recipe.count,
                cost: recipe.cost,
                workshop: recipe.workshop.map(WorkshopId::new),
            })?;
        }
        for (change, source) in &pack.changes {
            self.change(change, source)?;
        }
        for (change, source) in &pack.selections {
            self.change_items(change, source)?;
        }
        for (change, source) in &pack.recipe_selections {
            self.change_recipes(change, source)?;
        }
        let group = group(&pack.namespace);
        for drop in &pack.drops {
            self.add_loot_in(group, drop.clone())?;
        }
        #[cfg(feature = "unstable")]
        for reward in &pack.rewards {
            // A reward this build cannot follow turns off alone.
            if let Err(off) = self.add_reward(reward.reward(&toml::Table::new())) {
                crate::warn!("{REWARDS_FILE}: a reward is off: {off}");
            }
        }
        #[cfg(not(feature = "unstable"))]
        if !pack.rewards.is_empty() {
            return Err(Off::new(format!(
                "{REWARDS_FILE}: rewards need enshwick's `unstable` feature"
            )));
        }
        #[cfg(feature = "unstable")]
        for sound in &pack.sounds {
            // A sound whose file does not read turns off alone.
            let new = crate::sound::NewSound {
                scene: sound
                    .scene
                    .map_or_else(Default::default, sounds::SceneEntry::scene),
                container: crate::sound::SoundChange {
                    volume: sound.volume,
                    ..crate::sound::SoundChange::default()
                },
                ..crate::sound::NewSound::new(id(&sound.id)?, sound.file.clone())
            };
            let _added = self.feature(&format!("sound {}", sound.id), |load| load.add_sound(new));
        }
        #[cfg(not(feature = "unstable"))]
        if !pack.sounds.is_empty() {
            return Err(Off::new(format!(
                "{SOUNDS_FILE}: sounds need enshwick's `unstable` feature"
            )));
        }
        self.follow_pack_settings(pack)
    }

    /// Declares the settings of `pack.toml`, when it has some, as the mod's
    /// settings, and sets the numbers of `loot.toml` and `rewards.toml`
    /// that follow them as each world's settings arrive.
    fn follow_pack_settings(&mut self, pack: &Pack) -> Result<(), Off> {
        let Some(schema) = pack.settings_schema.clone() else {
            return Ok(());
        };
        let declared = self.table_config(schema)?;
        let settings = declared.0.clone();
        let me = self.host().handle();
        DECLARED
            .get_or_init(std::sync::Mutex::default)
            .relock()
            .0
            .push((me, declared));
        let drops: Vec<(crate::loot::Drop, values::Number<f64>)> = pack
            .drops
            .iter()
            .cloned()
            .zip(pack.drop_chances.iter().cloned())
            .collect();
        let drops_follow = drops.iter().any(|(_, chance)| chance.follows());
        #[cfg(feature = "unstable")]
        let rewards: Vec<rewards::PackReward> = pack
            .rewards
            .iter()
            .filter(|reward| reward.follows_settings())
            .cloned()
            .collect();
        let group = group(&pack.namespace);
        settings.on_change(self, move |values| {
            if drops_follow {
                let resolved = drops
                    .iter()
                    .map(|(drop, chance)| {
                        let mut drop = drop.clone();
                        drop.chance = chance.get(&values).clamp(0.0, 1.0);
                        drop
                    })
                    .collect();
                if let Err(error) = crate::loot::replace_group(group, resolved) {
                    crate::warn!(
                        "{LOOT_FILE}: the world's chances apply at the next start: {error}"
                    );
                }
            }
            #[cfg(feature = "unstable")]
            for reward in &rewards {
                crate::rewards::replace(me, reward.reward(&values));
            }
        })
    }
}

/// The settings each pack declared, and their defaults, by the pack's
/// handle, for a script runtime of the same mod, which follows them instead
/// of declaring them again.
#[derive(Default)]
struct DeclaredByMod(Vec<(crate::abi::ModHandle, Declared)>);

static DECLARED: crate::fresh::Fresh<std::sync::Mutex<DeclaredByMod>> = crate::fresh::Fresh::new();

/// A pack's settings and their defaults, as `table_config` gives them.
type Declared = (crate::config::TableConfig, toml::Table);

/// The settings [`Load::add_pack`] declared for the pack `load` starts, and
/// their defaults; `None` when it declared none.
#[doc(hidden)]
#[must_use]
pub fn declared_settings(load: &Load<'_>) -> Option<(crate::config::TableConfig, toml::Table)> {
    let me = load.host().handle();
    DECLARED
        .get_or_init(std::sync::Mutex::default)
        .relock()
        .0
        .iter()
        .find(|(handle, _)| *handle == me)
        .map(|(_, declared)| declared.clone())
}

/// The names of `names` closest to `wanted` within two edits, ignoring
/// case: the ones a misspelt name is near.
fn near<'a>(names: impl IntoIterator<Item = &'a str>, wanted: &str) -> Vec<&'a str> {
    let wanted = wanted.to_lowercase();
    let edits: Vec<(&str, usize)> = names
        .into_iter()
        .map(|name| (name, strsim::levenshtein(&name.to_lowercase(), &wanted)))
        .filter(|&(_, edits)| edits <= 2)
        .collect();
    let closest = edits.iter().map(|&(_, edits)| edits).min();
    edits
        .into_iter()
        .filter(|&(_, edits)| Some(edits) == closest)
        .map(|(name, _)| name)
        .collect()
}

/// `; near: ` and `near`, or nothing when it is empty.
fn near_names(near: &[&str]) -> String {
    if near.is_empty() {
        String::new()
    } else {
        format!("; near: {}", near.join(", "))
    }
}

mod changes;
mod entries;
mod files;
mod manifest;
mod pack;
mod points;
mod reader;
mod rewards;
mod sounds;
#[cfg(test)]
mod tests;
mod values;

#[cfg(feature = "schema")]
pub use entries::schemas;
pub use files::*;
pub use manifest::*;
pub use pack::*;
pub use points::{PackPoint, PackPointField, PointLayout, PointSlot};
