use std::collections::BTreeMap;

use crate::Off;
use crate::content::changes::{Change, Entry, ItemsChange, RecipesChange};
use crate::content::icons::Tint;
use crate::content::items::StatFormat;
use crate::content::vanilla::Vanilla;
use crate::content::workshops::Station;
use crate::content::{Guid, Id};
use crate::i18n::Texts;
use crate::loot::Drop;

use super::entries::{FormatEntry, ItemsFile, LootFile, RecipesFile, WorkshopsFile};
use super::files::{PackError, PackFiles, ParsePackError};
use super::reader::{Reader, source_of};
use super::{ITEMS_FILE, LOOT_FILE, RECIPES_FILE, TEXTS_FILE, WORKSHOPS_FILE};

/// A reference in a pack, resolved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Ref {
    /// A name the pack defines, as its full id.
    Own,
    /// Another mod's content id, or a vanilla GUID.
    Elsewhere,
}

/// A pack item, ready to add.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PackItem {
    pub(super) id: String,
    pub(super) like: Guid,
    pub(super) name: Option<String>,
    pub(super) description: Option<String>,
    pub(super) debug_name: String,
    pub(super) meter: Option<(u16, u16)>,
    pub(super) stats: Vec<(String, i32, StatFormat)>,
    pub(super) icon: Option<PackIcon>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum PackIcon {
    Tint { like: Guid, tint: Tint },
    Png(Vec<u8>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PackRecipe {
    pub(super) id: String,
    pub(super) like: Guid,
    pub(super) output: Guid,
    pub(super) count: u32,
    pub(super) cost: u32,
    pub(super) workshop: Option<Guid>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct PackWorkshop {
    pub(super) id: String,
    pub(super) like: Station,
    pub(super) name: Option<String>,
    pub(super) description: Option<String>,
    pub(super) cost: Option<u32>,
    pub(super) debug_name: String,
}

/// A pack, read and checked: what [`Load::add_pack`](crate::Load::add_pack) adds.
///
/// # Examples
///
/// See the [module](crate::packs).
#[derive(Clone, Debug, PartialEq)]
pub struct Pack {
    pub(super) namespace: String,
    pub(super) items: Vec<PackItem>,
    pub(super) recipes: Vec<PackRecipe>,
    pub(super) workshops: Vec<PackWorkshop>,
    pub(super) drops: Vec<Drop>,
    pub(super) changes: Vec<(Change, String)>,
    pub(super) selections: Vec<(ItemsChange, String)>,
    pub(super) recipe_selections: Vec<(RecipesChange, String)>,
    pub(super) skipped: Vec<PackError>,
    /// The content ids of other mods the pack names, which the loader
    /// checks against their exports.
    pub(super) named: Vec<String>,
    pub(super) texts: String,
    pub(super) hash: u32,
}

impl Pack {
    /// Reads and checks the pack `files` in the namespace `namespace`, the
    /// id of the mod or pack that adds it, with the running game's vanilla
    /// names ([`Vanilla::game`]) when there is a game. When `files` holds a
    /// `pack.toml`, a reference `<alias>:<name>` names the content of the
    /// mod its `requires` gives that alias.
    ///
    /// # Errors
    ///
    /// Every problem found, each with its file, line and column: a file
    /// that is no TOML of its form, an unknown field, a name defined twice,
    /// a reference to a name neither the pack nor the game has, or that
    /// several vanilla items carry, a station that cannot be cloned, a text
    /// key without text, an icon the pack lacks, a chance outside 0 to 1.
    ///
    /// # Examples
    ///
    /// See the [module](crate::packs).
    pub fn parse(namespace: &str, files: &PackFiles) -> Result<Self, ParsePackError> {
        Self::parse_with(namespace, files, Vanilla::game())
    }

    /// Reads and checks a pack as [`Pack::parse`] does, with the vanilla
    /// names of `vanilla`; without it, a pack names vanilla content by GUID
    /// only and clones only the workbench.
    ///
    /// # Errors
    ///
    /// As [`Pack::parse`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let files = PackFiles::new().with("items.toml", "[[item]]\nid = \"torch\"\nlike = \"Tool_Torch01\"\n");
    /// let errors = Pack::parse_with("lights", &files, None).err().ok_or("parsed")?;
    /// assert!(errors.to_string().contains("items.toml:1"));
    /// # Ok::<(), &str>(())
    /// ```
    pub fn parse_with(
        namespace: &str,
        files: &PackFiles,
        vanilla: Option<&Vanilla>,
    ) -> Result<Self, ParsePackError> {
        let mut reader = Reader::new(namespace, files, vanilla);
        let texts_text = match files.text(TEXTS_FILE) {
            Ok(text) => text.unwrap_or("").to_owned(),
            Err(error) => {
                reader.errors.push(error);
                String::new()
            }
        };
        let texts = match toml::from_str::<BTreeMap<String, BTreeMap<String, String>>>(&texts_text)
        {
            Err(error) => {
                reader
                    .errors
                    .push(PackError::of(TEXTS_FILE, &texts_text, &error));
                None
            }
            Ok(_) => match Texts::parse(&texts_text) {
                Ok(texts) => Some(texts),
                Err(off) => {
                    reader.error(TEXTS_FILE, &texts_text, 0, off.to_string());
                    None
                }
            },
        };
        let (items_file, items_text): (ItemsFile, String) = reader.file(ITEMS_FILE);
        let (workshops_file, workshops_text): (WorkshopsFile, String) = reader.file(WORKSHOPS_FILE);
        let (recipes_file, recipes_text): (RecipesFile, String) = reader.file(RECIPES_FILE);
        let (loot_file, loot_text): (LootFile, String) = reader.file(LOOT_FILE);
        for entry in &items_file.item {
            let at = entry.span().start;
            reader.define(ITEMS_FILE, &items_text, at, &entry.get_ref().id, "item");
        }
        for entry in &workshops_file.workshop {
            let at = entry.span().start;
            reader.define(
                WORKSHOPS_FILE,
                &workshops_text,
                at,
                &entry.get_ref().id,
                "workshop",
            );
        }
        for entry in &recipes_file.recipe {
            let at = entry.span().start;
            reader.define(
                RECIPES_FILE,
                &recipes_text,
                at,
                &entry.get_ref().id,
                "recipe",
            );
        }
        let mut pack = Self {
            namespace: namespace.to_owned(),
            items: Vec::new(),
            recipes: Vec::new(),
            workshops: Vec::new(),
            drops: Vec::new(),
            changes: Vec::new(),
            selections: Vec::new(),
            recipe_selections: Vec::new(),
            skipped: Vec::new(),
            named: Vec::new(),
            texts: texts_text,
            hash: files.hash(),
        };
        pack.items = items(&mut reader, &items_file, &items_text, texts.as_ref());
        pack.workshops = workshops(
            &mut reader,
            &workshops_file,
            &workshops_text,
            texts.as_ref(),
        );
        pack.recipes = recipes(&mut reader, &recipes_file, &recipes_text);
        pack.drops = drops(&mut reader, &loot_file, &loot_text, &pack.items);
        (pack.changes, pack.selections) =
            super::changes::item_changes(&mut reader, &items_file, &items_text);
        pack.changes.extend(super::changes::loot_changes(
            &mut reader,
            &loot_file,
            &loot_text,
        ));
        let (recipe_changes, recipe_selections) =
            super::changes::recipe_changes(&mut reader, &recipes_file, &recipes_text);
        pack.changes.extend(recipe_changes);
        pack.recipe_selections = recipe_selections;
        pack.skipped = reader.skipped;
        pack.named = reader.named.into_iter().collect();
        if reader.errors.is_empty() {
            Ok(pack)
        } else {
            Err(ParsePackError(reader.errors))
        }
    }

    /// The content ids the pack defines, in the order it adds them.
    ///
    /// # Examples
    ///
    /// See the [module](crate::packs).
    #[must_use]
    pub fn ids(&self) -> Vec<&str> {
        self.workshops
            .iter()
            .map(|w| w.id.as_str())
            .chain(self.items.iter().map(|i| i.id.as_str()))
            .chain(self.recipes.iter().map(|r| r.id.as_str()))
            .collect()
    }

    /// The pack's loot drops.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let files = PackFiles::new().with("loot.toml", r#"
    /// [[drop]]
    /// items = ["you.lockers:locker"]
    /// chance = 0.1
    /// from = "bosses"
    /// "#);
    /// let pack = Pack::parse("you.lanterns", &files)?;
    /// assert_eq!(pack.drops().len(), 1);
    /// # Ok::<(), enshwick::packs::ParsePackError>(())
    /// ```
    #[must_use]
    pub fn drops(&self) -> &[Drop] {
        &self.drops
    }

    /// The parts of changes left out because the running game no longer has
    /// the item or recipe they name, one line each for the log.
    ///
    /// A game update that drops one name skips only what names it; the rest
    /// of the pack applies.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let files = PackFiles::new().with("items.toml", "[[item]]\nid = \"torch\"\nlike = \"00000000-0000-0000-0000-000000000001\"\n");
    /// let pack = Pack::parse_with("lights", &files, None).map_err(|e| e.to_string())?;
    /// assert!(pack.skipped().is_empty());
    /// # Ok::<(), String>(())
    /// ```
    #[must_use]
    pub fn skipped(&self) -> &[PackError] {
        &self.skipped
    }

    /// The pack's changes of vanilla values, each with its source: the file
    /// and line, then the name the pack wrote unless it is a GUID, such as
    /// `items.toml:12 (Material_T1_Wood_Logs)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::{Field, Value};
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let files = PackFiles::new().with("items.toml", r#"
    /// [[change]]
    /// items = ["6943053a-32c1-4718-ae22-e61427ddf54d"]
    /// max_stack = { times = 4 }
    /// "#);
    /// let pack = Pack::parse("you.stacks", &files)?;
    /// let (change, source) = &pack.changes()[0];
    /// assert_eq!((change.field, change.value), (Field::MaxStackSize, Value::Times(4.0)));
    /// assert_eq!(source, "items.toml:2");
    /// # Ok::<(), enshwick::packs::ParsePackError>(())
    /// ```
    #[must_use]
    pub fn changes(&self) -> &[(Change, String)] {
        &self.changes
    }

    /// The pack's changes of every vanilla item a selection picks, each with
    /// its source: the file and line, then the selection.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::Items;
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let files = PackFiles::new().with("items.toml", "[[change]]\nitems = \"stackable\"\nmax_stack = { times = 2 }\n");
    /// let pack = Pack::parse("you.stacks", &files)?;
    /// let (change, source) = &pack.selections()[0];
    /// assert_eq!(change.items, Items::Stackable);
    /// assert_eq!(source, "items.toml:1 (stackable)");
    /// # Ok::<(), enshwick::packs::ParsePackError>(())
    /// ```
    #[must_use]
    pub fn selections(&self) -> &[(ItemsChange, String)] {
        &self.selections
    }

    /// The pack's changes of every vanilla recipe a selection picks, each
    /// with its source: the file and line, then the selection.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::changes::Recipes;
    /// use enshwick::content::recipes::RecipeKind;
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let files = PackFiles::new().with("recipes.toml", "[[change]]\nkind = [\"roof\"]\ncost = { times = 0.5 }\n");
    /// let pack = Pack::parse("you.roofs", &files)?;
    /// let (change, source) = &pack.recipe_selections()[0];
    /// assert_eq!(change.recipes, Recipes::Kind(RecipeKind::Roof));
    /// assert_eq!(source, "recipes.toml:1 (kind roof)");
    /// # Ok::<(), enshwick::packs::ParsePackError>(())
    /// ```
    #[must_use]
    pub fn recipe_selections(&self) -> &[(RecipesChange, String)] {
        &self.recipe_selections
    }

    /// Whether the pack adds or changes anything players' games read: items,
    /// workshops, recipes, texts or changes of values beside loot. Such a pack runs on
    /// the server and every player alike, so it keeps `join` on.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let loot = PackFiles::new().with("loot.toml", r#"
    /// [[drop]]
    /// items = ["you.lockers:locker"]
    /// chance = 0.1
    /// from = "bosses"
    /// "#);
    /// assert!(!Pack::parse("you.loot", &loot)?.reaches_players());
    /// # Ok::<(), enshwick::packs::ParsePackError>(())
    /// ```
    #[must_use]
    pub fn reaches_players(&self) -> bool {
        !(self.items.is_empty()
            && self.workshops.is_empty()
            && self.recipes.is_empty()
            && self
                .changes
                .iter()
                .all(|(change, _)| change.field.entry() == Entry::Loot)
            && self.selections.is_empty()
            && self.recipe_selections.is_empty()
            && self.texts.trim().is_empty())
    }

    /// Whether `other` adds the same content as this pack, its loot aside:
    /// what a running game can take without a restart.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let loot = |chance: &str| PackFiles::new().with("loot.toml", format!(
    ///     "[[drop]]\nitems = [\"you.lockers:locker\"]\nchance = {chance}\nfrom = \"bosses\"\n"
    /// ));
    /// let before = Pack::parse("you.lanterns", &loot("0.1"))?;
    /// let after = Pack::parse("you.lanterns", &loot("0.5"))?;
    /// assert!(before.same_content(&after));
    /// # Ok::<(), enshwick::packs::ParsePackError>(())
    /// ```
    #[must_use]
    pub fn same_content(&self, other: &Self) -> bool {
        self.namespace == other.namespace
            && self.items == other.items
            && self.recipes == other.recipes
            && self.workshops == other.workshops
            && self.changes == other.changes
            && self.selections == other.selections
            && self.recipe_selections == other.recipe_selections
            && self.texts == other.texts
    }

    /// Whether `other` adds the same content as this pack, its loot and its
    /// changes of values aside: what a development reload takes without a
    /// restart.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let stacks = |size: u32| PackFiles::new().with("items.toml", format!(
    ///     "[[change]]\nitems = [\"3f4093ae-75ed-4e5d-bd0b-784ba83b88f5\"]\nmax_stack = {size}\n"
    /// ));
    /// let before = Pack::parse("you.stacks", &stacks(100))?;
    /// let after = Pack::parse("you.stacks", &stacks(500))?;
    /// assert!(!before.same_content(&after));
    /// assert!(before.same_beside_changes(&after));
    /// # Ok::<(), enshwick::packs::ParsePackError>(())
    /// ```
    #[must_use]
    pub fn same_beside_changes(&self, other: &Self) -> bool {
        self.namespace == other.namespace
            && self.items == other.items
            && self.recipes == other.recipes
            && self.workshops == other.workshops
            && self.texts == other.texts
    }

    /// The hash of the files the pack was read from, as
    /// [`PackFiles::hash`] gives it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::{Pack, PackFiles};
    ///
    /// let files = PackFiles::new();
    /// assert_eq!(Pack::parse("empty", &files)?.hash(), files.hash());
    /// # Ok::<(), enshwick::packs::ParsePackError>(())
    /// ```
    #[must_use]
    pub fn hash(&self) -> u32 {
        self.hash
    }
}

/// The checked items of `items_file`.
pub(super) fn items(
    reader: &mut Reader<'_>,
    items_file: &ItemsFile,
    items_text: &str,
    texts: Option<&Texts>,
) -> Vec<PackItem> {
    let files = reader.files;
    let mut out = Vec::new();
    let file = ITEMS_FILE;
    let text = items_text;
    for entry in &items_file.item {
        let at = entry.span().start;
        let e = entry.get_ref();
        let like = reader.reference(file, text, at, &e.like, "item", "");
        let name = e
            .name
            .as_deref()
            .and_then(|k| reader.text_key(file, text, at, k, texts));
        let description = e
            .description
            .as_deref()
            .and_then(|k| reader.text_key(file, text, at, k, texts));
        let meter = e.meter.map(|m| (m.max, m.start.unwrap_or(m.max)));
        if let Some((max, start)) = meter
            && (max == 0 || start > max)
        {
            reader.error(
                file,
                text,
                at,
                format!(
                    "item {}: a meter holds 1 or more and starts at most full",
                    e.id
                ),
            );
        }
        let mut stats = Vec::new();
        for stat in &e.stats {
            if let Some(label) = reader.text_key(file, text, at, &stat.label, texts) {
                let format = match stat.format {
                    FormatEntry::Number => StatFormat::Number,
                    FormatEntry::Signed => StatFormat::Signed,
                    FormatEntry::Percent => StatFormat::Percent,
                };
                stats.push((label, stat.value, format));
            }
        }
        let icon = e
            .icon
            .as_ref()
            .and_then(|icon| match (&icon.tint, &icon.png) {
                (Some(colour), None) => {
                    let like = match &icon.like {
                        Some(r) => reader.reference(file, text, at, r, "item", "")?.0,
                        None => like?.0,
                    };
                    let amount = icon.amount.unwrap_or(50).min(100);
                    Some(PackIcon::Tint {
                        like,
                        tint: Tint {
                            colour: *colour,
                            amount,
                        },
                    })
                }
                (None, Some(png)) => match files.get(png) {
                    Some(bytes) if png.starts_with("icons/") => Some(PackIcon::Png(bytes.to_vec())),
                    _ => {
                        reader.error(
                            file,
                            text,
                            at,
                            format!("item {}: no icon {png:?} under icons/", e.id),
                        );
                        None
                    }
                },
                _ => {
                    reader.error(
                        file,
                        text,
                        at,
                        format!("item {}: an icon has either tint or png", e.id),
                    );
                    None
                }
            });
        if let Some((like, _)) = like {
            out.push(PackItem {
                id: reader.own(&e.id),
                like,
                name,
                description,
                debug_name: e.debug_name.clone().unwrap_or_else(|| reader.own(&e.id)),
                meter,
                stats,
                icon,
            });
        }
    }
    out
}

/// The checked workshops of `workshops_file`.
pub(super) fn workshops(
    reader: &mut Reader<'_>,
    workshops_file: &WorkshopsFile,
    workshops_text: &str,
    texts: Option<&Texts>,
) -> Vec<PackWorkshop> {
    let mut out = Vec::new();
    let file = WORKSHOPS_FILE;
    let text = workshops_text;
    for entry in &workshops_file.workshop {
        let at = entry.span().start;
        let e = entry.get_ref();
        let like = reader.station(file, text, at, &e.id, &e.like);
        let name = e
            .name
            .as_deref()
            .and_then(|k| reader.text_key(file, text, at, k, texts));
        let description = e
            .description
            .as_deref()
            .and_then(|k| reader.text_key(file, text, at, k, texts));
        if let Some(like) = like {
            out.push(PackWorkshop {
                id: reader.own(&e.id),
                like,
                name,
                description,
                cost: e.recipe.map(|r| r.cost),
                debug_name: e.debug_name.clone().unwrap_or_else(|| reader.own(&e.id)),
            });
        }
    }
    out
}

/// The checked recipes of `recipes_file`.
pub(super) fn recipes(
    reader: &mut Reader<'_>,
    recipes_file: &RecipesFile,
    recipes_text: &str,
) -> Vec<PackRecipe> {
    let mut out = Vec::new();
    let file = RECIPES_FILE;
    let text = recipes_text;
    for entry in &recipes_file.recipe {
        let at = entry.span().start;
        let e = entry.get_ref();
        let like = reader.reference(file, text, at, &e.like, "item", "");
        let output = reader.reference(file, text, at, &e.output, "item", "");
        let workshop = e
            .workshop
            .as_deref()
            .map(|w| reader.reference(file, text, at, w, "workshop", ".workshop"));
        if e.count == 0 || e.cost == 0 {
            reader.error(
                file,
                text,
                at,
                format!("recipe {}: count and cost are 1 or more", e.id),
            );
        }
        let workshop = match workshop {
            None => Some(None),
            Some(Some((guid, _))) => Some(Some(guid)),
            Some(None) => None,
        };
        if let (Some((like, _)), Some((output, _)), Some(workshop)) = (like, output, workshop) {
            out.push(PackRecipe {
                id: reader.own(&e.id),
                like,
                output,
                count: e.count,
                cost: e.cost,
                workshop,
            });
        }
    }
    out
}

/// The checked drops of `loot`.
pub(super) fn drops(
    reader: &mut Reader<'_>,
    loot: &LootFile,
    text: &str,
    items: &[PackItem],
) -> Vec<Drop> {
    let file = LOOT_FILE;
    let mut drops = Vec::new();
    for entry in &loot.drop {
        let at = entry.span().start;
        let e = entry.get_ref();
        let mut ids = Vec::new();
        let mut meter_of = Vec::new();
        for item in &e.items {
            if let Some((guid, kind)) = reader.reference(file, text, at, item, "item", "") {
                ids.push(crate::ItemId::new(guid.fnv1a()));
                let own = (kind == Ref::Own)
                    .then(|| items.iter().find(|i| Guid::try_named(&i.id) == Some(guid)))
                    .flatten();
                meter_of.push(own.and_then(|i| i.meter).map(|(max, _)| max));
            }
        }
        if e.items.is_empty() {
            reader.error(file, text, at, "a drop names no item");
        }
        if !(0.0..=1.0).contains(&e.chance) {
            reader.error(
                file,
                text,
                at,
                format!("chance {} is not between 0 and 1", e.chance),
            );
        }
        let from = match source_of(&e.from) {
            Ok(from) => from,
            Err(why) => {
                reader.error(file, text, at, why.to_string());
                continue;
            }
        };
        let [min, max] = e.count.unwrap_or([1, 1]);
        if min == 0 || min > max {
            reader.error(file, text, at, format!("count {min}..{max} is empty"));
        }
        let mut drop = Drop::one_of(&ids, e.chance, from).count(min, max);
        if let Some([low, high]) = e.meter {
            match meter_of.first().copied().flatten() {
                Some(of) if meter_of.iter().all(|m| *m == Some(of)) && low <= high && high <= of => {
                    drop = drop.meter(low, high, of);
                }
                _ => reader.error(
                    file,
                    text,
                    at,
                    format!("meter {low}..{high}: every item must be one of the pack's with a meter of the same maximum, at least {high}"),
                ),
            }
        }
        if ids.len() == e.items.len() {
            drops.push(drop);
        }
    }
    drops
}

/// A content id the pack built at run time; ids live for the process.
pub(super) fn id(full: &str) -> Result<Id, Off> {
    let name: &'static str = Box::leak(full.to_owned().into_boxed_str());
    Id::try_new(name).ok_or_else(|| Off::new(format!("data pack: {full:?} is no content id")))
}

/// The group of a pack's drops, which a reload replaces.
#[must_use]
pub(crate) fn group(namespace: &str) -> u64 {
    namespace
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, b| {
            (hash ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
        })
        | 1
}

/// Replaces the drops of the pack `namespace` with `drops`, as the loader
/// does when a pack's `loot.toml` changes.
///
/// # Errors
///
/// Why they take a restart instead.
#[doc(hidden)]
pub fn reload_loot(
    namespace: &str,
    drops: Vec<Drop>,
) -> Result<(), crate::loot::ReplaceDropsError> {
    crate::loot::replace_group(group(namespace), drops)
}
