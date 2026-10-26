//! Reading a pack's entries into what it adds, noting every problem.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::entries::{FormatEntry, FromEntry, ItemsFile, LootFile, RecipesFile, WorkshopsFile};
use super::{
    ITEMS_FILE, LOOT_FILE, PackError, PackFiles, PackIcon, PackItem, PackRecipe, PackWorkshop,
    RECIPES_FILE, Ref, TEXTS_FILE, WORKSHOPS_FILE,
};
use crate::content::Guid;
use crate::content::icons::Tint;
use crate::content::items::StatFormat;
use crate::content::vanilla::Vanilla;
use crate::content::workshops::{Station, WorkshopId, stations};
use crate::i18n::Texts;
use crate::loot::{Drop, Kinds, Source};

/// Collects a pack's problems as it is read.
pub(super) struct Reader<'a> {
    pub(super) namespace: &'a str,
    pub(super) files: &'a PackFiles,
    pub(super) errors: Vec<PackError>,
    /// Names the pack defines, with what each is.
    pub(super) defined: BTreeMap<String, &'static str>,
    /// The game's content, for vanilla names.
    pub(super) vanilla: Option<&'a Vanilla>,
}

impl Reader<'_> {
    /// The parsed `name`, or `T::default` when the pack lacks it; problems
    /// are noted.
    pub(super) fn file<T: for<'de> Deserialize<'de> + Default>(
        &mut self,
        name: &str,
    ) -> (T, String) {
        let text = match self.files.text(name) {
            Ok(Some(text)) => text.to_owned(),
            Ok(None) => return (T::default(), String::new()),
            Err(error) => {
                self.errors.push(error);
                return (T::default(), String::new());
            }
        };
        match toml::from_str(&text) {
            Ok(parsed) => (parsed, text),
            Err(error) => {
                self.errors.push(PackError::of(name, &text, &error));
                (T::default(), text)
            }
        }
    }

    pub(super) fn error(&mut self, file: &str, text: &str, at: usize, message: impl AsRef<str>) {
        self.errors
            .push(PackError::at(file, text, at, message.as_ref()));
    }

    /// The full id of the pack's own `name`: `<namespace>:<name>`.
    pub(super) fn own(&self, name: &str) -> String {
        format!("{}:{name}", self.namespace)
    }

    /// Notes that the pack defines `name` as a `what`; a problem when it
    /// is no valid name or is defined already.
    pub(super) fn define(
        &mut self,
        file: &str,
        text: &str,
        at: usize,
        name: &str,
        what: &'static str,
    ) {
        let full = self.own(name);
        if name.contains(':') || Guid::try_named(&full).is_none() {
            self.error(
                file,
                text,
                at,
                format!("{what} id {name:?} is no name: 1 to 128 of [a-z0-9_./-]"),
            );
            return;
        }
        if let Some(earlier) = self.defined.insert(full, what) {
            self.error(
                file,
                text,
                at,
                format!("{what} id {name:?} is taken by a {earlier} already"),
            );
        }
    }

    /// The GUID `reference` names, and whether the pack defines it; a
    /// pack name must be a `what` of the pack.
    pub(super) fn reference(
        &mut self,
        file: &str,
        text: &str,
        at: usize,
        reference: &str,
        what: &'static str,
        suffix: &str,
    ) -> Option<(Guid, Ref)> {
        if let Some(guid) = Guid::try_parse(reference) {
            return Some((guid, Ref::Elsewhere));
        }
        if reference.contains(':') {
            let guid = Guid::try_named(reference);
            if guid.is_none() {
                self.error(
                    file,
                    text,
                    at,
                    format!("{reference:?} is no content id: <mod>:<name>"),
                );
            }
            return guid.map(|g| (g, Ref::Elsewhere));
        }
        let full = self.own(reference);
        if self.defined.get(&full) == Some(&what) {
            return Guid::try_named(&format!("{full}{suffix}")).map(|g| (g, Ref::Own));
        }
        let found = match self.vanilla {
            None => Err(format!(
                "no {what} {reference:?} in this pack; name another mod's as <mod>:<name>, or a vanilla one by its GUID or, where the game's data is at hand, its debug name"
            )),
            Some(vanilla) => vanilla
                .item(reference)
                .and_then(|item| match what {
                    "workshop" => vanilla.workshop(item).map(WorkshopId::guid),
                    _ => Ok(item),
                })
                .map_err(|why| format!("no {what} {reference:?} in this pack, and {why}")),
        };
        match found {
            Ok(guid) => Some((guid, Ref::Elsewhere)),
            Err(why) => {
                self.error(file, text, at, why);
                None
            }
        }
    }

    /// The vanilla station `like` names: `workbench`, a station item's debug
    /// name or its GUID.
    pub(super) fn station(
        &mut self,
        file: &str,
        text: &str,
        at: usize,
        id: &str,
        like: &str,
    ) -> Option<Station> {
        if like == "workbench" {
            return Some(stations::WORKBENCH);
        }
        let found = match (self.vanilla, Guid::try_parse(like)) {
            (_, Some(item)) if item == stations::WORKBENCH.item => Ok(stations::WORKBENCH),
            (None, _) => Err(format!(
                "like {like:?}: a station other than the workbench needs the game's data at hand"
            )),
            (Some(vanilla), Some(item)) => vanilla.station(item).map_err(|why| why.to_string()),
            (Some(vanilla), None) => vanilla
                .item(like)
                .and_then(|item| vanilla.station(item))
                .map_err(|why| why.to_string()),
        };
        found
            .map_err(|why| self.error(file, text, at, format!("workshop {id}: {why}")))
            .ok()
    }

    /// A text key the pack's texts must hold.
    pub(super) fn text_key(
        &mut self,
        file: &str,
        text: &str,
        at: usize,
        key: &str,
        texts: Option<&Texts>,
    ) -> Option<String> {
        let tag = self.own(&format!("text/{key}"));
        if Guid::try_named(&tag).is_none() {
            self.error(
                file,
                text,
                at,
                format!("text key {key:?} is no name: 1 to 123 of [a-z0-9_./-]"),
            );
            return None;
        }
        if texts.is_some_and(|t| t.translations(key).next().is_none()) {
            self.error(
                file,
                text,
                at,
                format!("text key {key:?} has no text in {TEXTS_FILE}"),
            );
        }
        Some(key.to_owned())
    }
}

/// The kinds and filters of a drop's `from`.
pub(super) fn source_of(from: &FromEntry) -> Result<Source, String> {
    let kind = |name: &str| match name {
        "corpses" => Some(Kinds::CORPSES),
        "chests" => Some(Kinds::CHESTS),
        "containers" => Some(Kinds::CONTAINERS),
        "destroyed" => Some(Kinds::DESTROYED),
        _ => None,
    };
    let unknown = |name: &str| {
        format!("from {name:?}: bosses, locked-chests, corpses, chests, containers or destroyed")
    };
    match from {
        FromEntry::One(name) if name == "bosses" => Ok(Source::bosses()),
        FromEntry::One(name) if name == "locked-chests" => Ok(Source::locked_chests()),
        FromEntry::One(name) => kind(name).map(Source::kinds).ok_or_else(|| unknown(name)),
        FromEntry::Many(names) if names.is_empty() => Err("from names no kind".to_owned()),
        FromEntry::Many(names) => names
            .iter()
            .try_fold(Source::kinds(Kinds::default()), |s, n| {
                kind(n)
                    .map(|k| Source::kinds(s.kinds.with(k)))
                    .ok_or_else(|| unknown(n))
            }),
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
                ids.push(guid.id());
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
                reader.error(file, text, at, why);
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
