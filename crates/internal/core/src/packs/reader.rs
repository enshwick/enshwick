//! The reader of a pack's files, which collects its problems as it goes.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::abi::Relation;
use crate::content::Guid;
use crate::content::vanilla::{Vanilla, VanillaError};
use crate::content::workshops::{Station, WorkshopId, stations};
use crate::i18n::Texts;
use crate::loot::{Kinds, Source};

use super::entries::FromEntry;
use super::files::{PackError, PackFiles};
use super::manifest::PackManifest;
use super::pack::Ref;
use super::{MANIFEST_FILE, TEXTS_FILE};

/// What a vanilla name in a change names, for [`Reader::dropped`].
#[derive(Clone, Copy, Debug)]
pub(super) enum Dropped {
    Item,
    Recipe,
}

impl Dropped {
    const fn word(self) -> &'static str {
        match self {
            Self::Item => "item",
            Self::Recipe => "recipe",
        }
    }
}

/// Collects a pack's problems as it is read.
pub(super) struct Reader<'a> {
    pub(super) namespace: &'a str,
    pub(super) files: &'a PackFiles,
    pub(super) errors: Vec<PackError>,
    /// Parts of changes left out because the game's data no longer has
    /// the vanilla name they give.
    pub(super) skipped: Vec<PackError>,
    /// Names the pack defines, with what each is.
    pub(super) defined: BTreeMap<String, &'static str>,
    /// The game's content, for vanilla names.
    pub(super) vanilla: Option<&'a Vanilla>,
    /// The dependencies `pack.toml` names, by alias: their ids and
    /// relations.
    pub(super) aliases: BTreeMap<String, (String, Relation)>,
    /// The content ids of other mods the pack names.
    pub(super) named: std::collections::BTreeSet<String>,
}

impl<'a> Reader<'a> {
    /// A reader of the pack `namespace` in `files`, with its aliases read.
    pub(super) fn new(
        namespace: &'a str,
        files: &'a PackFiles,
        vanilla: Option<&'a Vanilla>,
    ) -> Self {
        let mut errors = Vec::new();
        let aliases = aliases(&mut errors, files);
        Self {
            namespace,
            files,
            errors,
            skipped: Vec::new(),
            defined: BTreeMap::new(),
            vanilla,
            aliases,
            named: std::collections::BTreeSet::new(),
        }
    }
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

    /// Whether `name` in change `number` is a vanilla debug name the game's
    /// data no longer has, with no name near it: an item or recipe a game
    /// update dropped. That part of the change is noted as skipped, so the
    /// rest of the pack still applies; a name near one the game has stays a
    /// problem, since it is likely misspelt.
    pub(super) fn dropped(
        &mut self,
        (file, text, at, number): (&str, &str, usize, usize),
        name: &str,
        what: Dropped,
    ) -> bool {
        let Some(vanilla) = self.vanilla else {
            return false;
        };
        if Guid::try_parse(name).is_some()
            || name.contains(':')
            || self.defined.contains_key(&self.own(name))
        {
            return false;
        }
        let gone = match what {
            Dropped::Item => matches!(
                vanilla.item(name),
                Err(VanillaError::Unknown { near, .. }) if near.is_empty()
            ),
            Dropped::Recipe => matches!(
                vanilla.recipe(name),
                Err(VanillaError::UnknownRecipe { near, .. }) if near.is_empty()
            ),
        };
        if gone {
            let message = format!(
                "change {number}: no vanilla {} {name:?} in this game build; skipped",
                what.word()
            );
            self.skipped.push(PackError::at(file, text, at, &message));
        }
        gone
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
        if let Some((owner, name)) = reference.split_once(':') {
            let full = match self.aliases.get(owner) {
                None => reference.to_owned(),
                Some((id, relation)) if *relation == Relation::REQUIRES => format!("{id}:{name}"),
                Some((id, _)) => {
                    self.error(
                        file,
                        text,
                        at,
                        format!(
                            "{reference:?}: {owner} is {id}, which pack.toml does not require, so it may not run; name its content once `requires` names it"
                        ),
                    );
                    return None;
                }
            };
            let guid = Guid::try_named(&full);
            if guid.is_none() {
                self.error(
                    file,
                    text,
                    at,
                    format!("{reference:?} is no content id: <alias>:<name> or <mod>:<name>"),
                );
            } else if full
                .split_once(':')
                .is_some_and(|(id, _)| id != self.namespace)
            {
                self.named.insert(full);
            }
            return guid.map(|g| (g, Ref::Elsewhere));
        }
        let full = self.own(reference);
        if self.defined.get(&full) == Some(&what) {
            return Guid::try_named(&format!("{full}{suffix}")).map(|g| (g, Ref::Own));
        }
        let found = match self.vanilla {
            None => Err(format!(
                "no {what} {reference:?} in this pack; name a required mod's as <alias>:<name>, or a vanilla one by its GUID or, where the game's data is at hand, its debug name"
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

/// The dependencies the `pack.toml` among `files` names, by alias, with
/// their ids and relations; none without one. A manifest that does not
/// read is noted in `errors`.
fn aliases(errors: &mut Vec<PackError>, files: &PackFiles) -> BTreeMap<String, (String, Relation)> {
    let manifest = match files.text(MANIFEST_FILE) {
        Ok(None) => return BTreeMap::new(),
        Ok(Some(text)) => PackManifest::parse(text),
        Err(error) => Err(error),
    };
    match manifest {
        Ok(manifest) => manifest
            .dependencies()
            .map(|(relation, alias, d)| (alias.to_owned(), (d.id.clone(), relation)))
            .collect(),
        Err(error) => {
            errors.push(error);
            BTreeMap::new()
        }
    }
}

/// The kinds and filters of a drop's `from`.
pub(super) fn source_of(from: &FromEntry) -> Result<Source, ReadSourceError> {
    let kind = |name: &str| match name {
        "corpses" => Some(Kinds::CORPSES),
        "chests" => Some(Kinds::CHESTS),
        "containers" => Some(Kinds::CONTAINERS),
        "destroyed" => Some(Kinds::DESTROYED),
        _ => None,
    };
    let unknown = |name: &str| ReadSourceError::Unknown {
        name: name.to_owned(),
    };
    match from {
        FromEntry::One(name) if name == "bosses" => Ok(Source::bosses()),
        FromEntry::One(name) if name == "locked-chests" => Ok(Source::locked_chests()),
        FromEntry::One(name) => kind(name).map(Source::kinds).ok_or_else(|| unknown(name)),
        FromEntry::Many(names) if names.is_empty() => Err(ReadSourceError::Empty),
        FromEntry::Many(names) => names
            .iter()
            .try_fold(Source::kinds(Kinds::default()), |s, n| {
                kind(n)
                    .map(|k| Source::kinds(s.kinds.with(k)))
                    .ok_or_else(|| unknown(n))
            }),
    }
}

/// Why a drop's `from` names no source of loot.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(super) enum ReadSourceError {
    /// A name that is no kind of source.
    #[error("from {name:?}: bosses, locked-chests, corpses, chests, containers or destroyed")]
    Unknown { name: String },
    /// An empty list.
    #[error("from names no kind")]
    Empty,
}
