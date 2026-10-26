//! Adding a read pack through the calls code uses.

use super::{Pack, PackIcon, group};
use crate::content::icons::{Icon, Pixels};
use crate::content::items::{Meter, NewItem, Stat};
use crate::content::recipes::NewRecipe;
use crate::content::workshops::{NewWorkshop, StationRecipe, WorkshopId};
use crate::content::{Guid, Id};
use crate::{Load, Off};

/// A content id the pack built at run time; ids live for the process.
pub(super) fn id(full: &str) -> Result<Id, Off> {
    let name: &'static str = Box::leak(full.to_owned().into_boxed_str());
    Id::try_new(name).ok_or_else(|| Off::new(format!("data pack: {full:?} is no content id")))
}

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
                stats.push(Stat {
                    label,
                    value: *value,
                    format: *format,
                });
            }
            let meter = item
                .meter
                .map(|(max, start)| -> Result<Meter, Off> {
                    Ok(Meter {
                        template: id(&format!("{}.meter", item.id))?,
                        max,
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
            self.add_recipe(NewRecipe {
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
        let group = group(&pack.namespace);
        for drop in &pack.drops {
            self.add_loot_in(group, drop.clone())?;
        }
        Ok(())
    }
}
