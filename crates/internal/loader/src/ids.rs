//! Namespaced ids: content, stat kinds and affixes each named
//! `<mod>:<name>` and stored by a 32-bit key derived from the name. Two
//! names with one key would read as one thing, so the second is refused,
//! with both names and both mods in the reason.

use std::sync::Mutex;

use enshwick::content::Guid;
use rustc_hash::FxHashMap;

use crate::locks::Relock;

/// What an id names; each kind has keys of its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Space {
    /// Game content: items, recipes, workshops, templates, texts, icons;
    /// the key is the game id of the name's GUID.
    Content,
    /// Stat kinds.
    Stat,
    /// Affixes.
    Affix,
}

impl Space {
    fn noun(self) -> &'static str {
        match self {
            Self::Content => "content id",
            Self::Stat => "stat kind",
            Self::Affix => "affix",
        }
    }
}

/// The ids seen so far, by space and key: the name and the mod it came from.
#[derive(Debug, Default)]
pub(crate) struct Ids {
    known: FxHashMap<(Space, u32), (Box<str>, Box<str>)>,
}

impl Ids {
    /// Records `name` from mod `owner` under `key`; the reason when another
    /// name holds the key.
    pub(crate) fn declare(
        &mut self,
        owner: &str,
        space: Space,
        name: &str,
        key: u32,
    ) -> Result<(), String> {
        match self.known.get(&(space, key)) {
            Some((known, _)) if **known == *name => Ok(()),
            Some((known, by)) => Err(format!(
                "{noun} {name} of {owner} has the key {key:#010x} of {noun} {known} of {by}; \
                 one of them needs another name",
                noun = space.noun(),
            )),
            None => {
                self.known.insert((space, key), (name.into(), owner.into()));
                Ok(())
            }
        }
    }

    /// Every content id, sorted, with its mod.
    pub(crate) fn content(&self) -> Vec<(&str, &str)> {
        let mut all: Vec<(&str, &str)> = self
            .known
            .iter()
            .filter(|((space, _), _)| *space == Space::Content)
            .map(|(_, (name, by))| (&**name, &**by))
            .collect();
        all.sort_unstable();
        all
    }
}

static IDS: Mutex<Option<Ids>> = Mutex::new(None);

/// Runs `f` on the process's ids.
pub(crate) fn with<R>(f: impl FnOnce(&mut Ids) -> R) -> R {
    let mut ids = IDS.relock();
    f(ids.get_or_insert_with(Ids::default))
}

/// Declares content id `name` for the mod named `names`, its id first and
/// then its former ids, and returns its GUID: refused for a malformed id,
/// one outside the mod's namespaces, or one whose game id another content id
/// has. A renamed mod keeps its content under a former id, so the GUIDs
/// saved worlds hold stay.
pub(crate) fn declare_content(names: &[&str], name: &str) -> Result<Guid, String> {
    let owner = names.first().copied().unwrap_or_default();
    let guid = Guid::try_named(name).ok_or_else(|| format!("content id {name:?} is malformed"))?;
    if name
        .split_once(':')
        .is_none_or(|(namespace, _)| !names.contains(&namespace))
    {
        return Err(format!(
            "content id {name} lies outside the mod's namespace {owner}:"
        ));
    }
    with(|ids| ids.declare(owner, Space::Content, name, guid.id()))?;
    Ok(guid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_declared_again_is_the_same_id() {
        let mut ids = Ids::default();
        assert!(ids.declare("a", Space::Content, "a:x", 7).is_ok());
        assert!(ids.declare("a", Space::Content, "a:x", 7).is_ok());
        assert_eq!(ids.content(), [("a:x", "a")]);
    }

    #[test]
    fn two_names_with_one_key_name_both_mods() {
        let mut ids = Ids::default();
        assert!(ids.declare("a", Space::Content, "a:x", 7).is_ok());
        let why = ids.declare("b", Space::Content, "b:y", 7).unwrap_err();
        assert!(
            why.contains("b:y of b") && why.contains("a:x of a"),
            "{why}"
        );
    }

    #[test]
    fn spaces_keep_their_own_keys() {
        let mut ids = Ids::default();
        assert!(ids.declare("a", Space::Content, "a:x", 7).is_ok());
        assert!(ids.declare("b", Space::Stat, "b:y", 7).is_ok());
    }

    #[test]
    fn content_stays_in_its_mod_s_namespace() {
        assert_eq!(
            declare_content(&["ids-test"], "ids-test:lamp").ok(),
            Some(Guid::named("ids-test:lamp"))
        );
        assert!(declare_content(&["ids-test"], "other:lamp").is_err());
        assert!(declare_content(&["ids-test"], "lamp").is_err());
        assert!(declare_content(&["me.ids-test", "ids-test"], "ids-test:chair").is_ok());
    }
}
