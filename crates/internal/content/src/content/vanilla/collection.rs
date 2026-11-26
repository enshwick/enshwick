//! What the template collection names: the vanilla templates by their names,
//! and the fill tables their containers roll.

use std::collections::BTreeMap;

use super::super::loot_tables::template_table;
use super::super::templates::TEMPLATE_COLLECTION;
use super::{LootTable, Names, Vanilla, VanillaError, VanillaTemplate, named, near};
use crate::content::{Blob, Guid};

/// The fill tables containers roll: each container template's by its name,
/// and how many templates roll each table.
#[derive(Debug, Default)]
pub(super) struct FillTables {
    by_name: Names,
    shared: BTreeMap<[u8; 16], usize>,
}

impl Vanilla {
    /// The vanilla template named `name`, as the template collection lists
    /// it. The first call reads every template's name once, 139 ms on a
    /// dedicated server (build 26083076), more than a Luau script's budget
    /// of 100 ms a second: name templates while the mod starts. A Rust mod's
    /// first call after `init` logs a warning saying so.
    ///
    /// # Errors
    ///
    /// [`VanillaError::UnknownTemplate`] with the names that look like it,
    /// [`VanillaError::AmbiguousTemplate`] when several templates carry it,
    /// and [`VanillaError::Unreadable`] when the templates do not read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::vanilla::Vanilla;
    ///
    /// if let Some(vanilla) = Vanilla::game() {
    ///     let _wolf = vanilla.template("Enemy_Wildbeast_Wolf");
    /// }
    /// ```
    pub fn template(&self, name: &str) -> Result<Guid, VanillaError> {
        self.template_named(name, true, false)
    }

    /// Every vanilla template with the name the template collection gives
    /// it, sorted by name, as [`templates`](super::templates) lists them.
    /// The first call reads every template's name once, as
    /// [`Vanilla::template`] does, without its warning.
    ///
    /// # Errors
    ///
    /// [`VanillaError::Unreadable`] when the templates do not read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::vanilla::Vanilla;
    ///
    /// if let Some(vanilla) = Vanilla::game() {
    ///     let wolves = vanilla.templates()?.iter().filter(|t| t.name.contains("Wolf")).count();
    ///     enshwick::info!("{wolves} wolf templates");
    /// }
    /// # Ok::<(), enshwick::content::vanilla::VanillaError>(())
    /// ```
    pub fn templates(&self) -> Result<Vec<VanillaTemplate>, VanillaError> {
        Ok(self
            .template_names(false)?
            .iter()
            .flat_map(|(name, guids)| {
                guids.iter().map(|guid| VanillaTemplate {
                    guid: *guid,
                    name: name.clone(),
                })
            })
            .collect())
    }

    /// [`Vanilla::template`], warning a mod that reads the names after init
    /// when `warn`, and also taking a name in another case when `any_case`,
    /// as [`named`] reads it.
    pub(crate) fn template_named(
        &self,
        name: &str,
        warn: bool,
        any_case: bool,
    ) -> Result<Guid, VanillaError> {
        let names = self.template_names(warn)?;
        match &*named(names, name, any_case) {
            [one] => Ok(*one),
            [] => Err(VanillaError::UnknownTemplate {
                name: name.to_owned(),
                near: near(names, name),
            }),
            several => Err(VanillaError::AmbiguousTemplate {
                name: name.to_owned(),
                templates: several.to_vec(),
            }),
        }
    }

    /// The template names, read once; a mod that reads them first after
    /// init is warned when `warn`.
    pub(crate) fn template_names(&self, warn: bool) -> Result<&Names, VanillaError> {
        self.template_names
            .get_or_init(|| {
                if warn && crate::__private::initialised() {
                    crate::warn_once!(
                        "vanilla templates named after init: every template's name is read now, on this thread, about 139 ms on a dedicated server; name them in init"
                    );
                }
                self.read_template_names()
            })
            .as_ref()
            .ok_or(VanillaError::Unreadable("templates"))
    }

    fn read_template_names(&self) -> Option<Names> {
        let at = self.template_name?;
        let collection =
            (self.read)(TEMPLATE_COLLECTION.key_by_hash(self.hashes.template_collection))?;
        let (start, count) = Blob::new(&collection).array(self.workshops.templates)?;
        let mut names = Names::new();
        for at_guid in (0..usize::try_from(count).ok()?).map(|i| start + i * 16) {
            let guid = Guid(collection.get(at_guid..at_guid + 16)?.try_into().ok()?);
            let Some(template) = (self.read)(guid.key_by_hash(self.hashes.template)) else {
                continue;
            };
            let blob = Blob::new(&template);
            let Some((text, len)) = blob.array(at) else {
                continue;
            };
            let Some(name) = template.get(text..text + usize::try_from(len).ok()?) else {
                continue;
            };
            let name = String::from_utf8_lossy(name)
                .trim_end_matches('\0')
                .to_owned();
            names.entry(name).or_default().push(guid);
        }
        Some(names)
    }

    /// The fill table the vanilla container template named `name` rolls,
    /// such as `LootContainer_Enemy_Corpse_Vukah`, with how many templates
    /// share it: a change of the table reaches all of them. The first call
    /// reads every template once.
    ///
    /// # Errors
    ///
    /// [`VanillaError::NoLootTable`] with the names that look like it, and
    /// [`VanillaError::Unreadable`] when the templates do not read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::vanilla::Vanilla;
    ///
    /// if let Some(vanilla) = Vanilla::game() {
    ///     let vukah = vanilla.loot_table("LootContainer_Enemy_Corpse_Vukah")?;
    ///     assert!(vukah.containers >= 1);
    /// }
    /// # Ok::<(), enshwick::content::vanilla::VanillaError>(())
    /// ```
    pub fn loot_table(&self, name: &str) -> Result<LootTable, VanillaError> {
        let tables = self
            .fill_tables
            .get_or_init(|| self.read_fill_tables())
            .as_ref()
            .ok_or(VanillaError::Unreadable("container templates"))?;
        match tables.by_name.get(name).and_then(|tables| tables.first()) {
            Some(table) => Ok(LootTable {
                table: *table,
                containers: tables.shared.get(&table.0).copied().unwrap_or(1),
            }),
            None => Err(VanillaError::NoLootTable {
                name: name.to_owned(),
                near: near(&tables.by_name, name),
            }),
        }
    }

    fn read_fill_tables(&self) -> Option<FillTables> {
        let layout = self.fill?;
        let collection =
            (self.read)(TEMPLATE_COLLECTION.key_by_hash(self.hashes.template_collection))?;
        let blob = Blob::new(&collection);
        let (start, count) = blob.array(self.workshops.templates)?;
        let mut tables = FillTables::default();
        for at in (0..usize::try_from(count).ok()?).map(|i| start + i * 16) {
            let guid: [u8; 16] = collection.get(at..at + 16)?.try_into().ok()?;
            let Some(template) = (self.read)(Guid(guid).key_by_hash(self.hashes.template)) else {
                continue;
            };
            if let Some((name, table)) = template_table(&layout, &template) {
                *tables.shared.entry(table.0).or_default() += 1;
                tables.by_name.entry(name).or_default().push(table);
            }
        }
        Some(tables)
    }
}
