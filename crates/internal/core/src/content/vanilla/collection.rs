//! What the template collection names: the vanilla templates by their names,
//! and the fill tables their containers roll.

use std::collections::BTreeMap;

use super::super::loot_tables::template_table;
use super::super::templates::TEMPLATE_COLLECTION;
use super::{LootTable, Names, Vanilla, VanillaError, near};
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
    /// # extern crate enshwick_core as enshwick;
    /// use enshwick::content::vanilla::Vanilla;
    ///
    /// if let Some(vanilla) = Vanilla::game() {
    ///     let _wolf = vanilla.template("Enemy_Wildbeast_Wolf");
    /// }
    /// ```
    pub fn template(&self, name: &str) -> Result<Guid, VanillaError> {
        let names = self
            .template_names
            .get_or_init(|| {
                if crate::__private::initialised() {
                    crate::warn_once!(
                        "vanilla templates named after init: every template's name is read now, on this thread, about 139 ms on a dedicated server; name them in init"
                    );
                }
                self.read_template_names()
            })
            .as_ref()
            .ok_or(VanillaError::Unreadable("templates"))?;
        match names.get(name).map(Vec::as_slice) {
            Some([one]) => Ok(*one),
            Some(several) if !several.is_empty() => Err(VanillaError::AmbiguousTemplate {
                name: name.to_owned(),
                templates: several.to_vec(),
            }),
            _ => Err(VanillaError::UnknownTemplate {
                name: name.to_owned(),
                near: near(names, name),
            }),
        }
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
    /// # extern crate enshwick_core as enshwick;
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
