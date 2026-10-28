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
use crate::content::workshops::{NewWorkshop, StationRecipe, WorkshopId};
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
/// The texts file, in the form of a mod's [texts](crate::i18n).
pub const TEXTS_FILE: &str = "texts.toml";
/// Every file a pack folder may hold besides its icons, in the order the
/// loader reads them.
pub const FILES: [&str; 6] = [
    MANIFEST_FILE,
    ITEMS_FILE,
    RECIPES_FILE,
    WORKSHOPS_FILE,
    LOOT_FILE,
    TEXTS_FILE,
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
    /// the loader refuses.
    ///
    /// # Examples
    ///
    /// ```
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
        let texts = self.texts(&pack.texts)?;
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
            let base = &workshop.id;
            let name = tag(self, &workshop.name)?;
            let description = tag(self, &workshop.description)?;
            let station_recipe = workshop
                .cost
                .map(|cost| -> Result<StationRecipe, Off> {
                    Ok(StationRecipe {
                        id: id(&format!("{base}.recipe"))?,
                        cost,
                    })
                })
                .transpose()?;
            self.add_workshop(NewWorkshop {
                like: workshop.like,
                item: id(base)?,
                template: id(&format!("{base}.template"))?,
                workshop: id(&format!("{base}.workshop"))?,
                name,
                description,
                station_recipe,
                debug_name: workshop.debug_name.clone(),
            })?;
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
        Ok(())
    }
}

mod changes;
mod entries;
mod files;
mod manifest;
mod pack;
#[cfg(test)]
mod tests;

#[cfg(feature = "schema")]
pub use entries::schemas;
pub use files::*;
pub use manifest::*;
pub use pack::*;
