//! Namespaced ids: content, stat kinds and affixes each named
//! `<mod>:<name>` and stored by a 32-bit key derived from the name. Two
//! names with one key would read as one thing, so the second is refused,
//! with both names and both mods in the reason.

use std::sync::Mutex;

use enshwick_core::content::Guid;
use rustc_hash::FxHashMap;

use enshwick_base::Relock;

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

/// Why the loader refuses an id a mod declares.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum DeclareIdError {
    /// Another name of the space holds the name's key.
    #[error(
        "{noun} {name} of {owner} has the key {key:#010x} of {noun} {known} of {by}; one of them needs another name",
        noun = space.noun()
    )]
    Taken {
        space: Space,
        name: String,
        owner: String,
        key: u32,
        known: String,
        by: String,
    },
    /// The name is no content id.
    #[error("content id {name:?} is malformed")]
    Malformed { name: String },
    /// The name lies outside the mod's namespace.
    #[error("content id {name} lies outside the mod's namespace {owner}:")]
    Outside { name: String, owner: String },
}

/// The ids seen so far, by space and key: the name and the mod it came from.
#[derive(Debug, Default)]
pub(crate) struct Ids {
    known: FxHashMap<(Space, u32), (Box<str>, Box<str>)>,
    props: CraftingProps,
}

/// The crafting props mods add, in the order they were given their index:
/// the game's own count and the mask's bits, from the first call, and each
/// prop's GUID.
#[derive(Debug, Default)]
struct CraftingProps {
    bounds: Option<(u8, u8)>,
    added: Vec<Guid>,
}

/// Why the loader refuses a crafting prop's index.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CraftingPropError {
    /// The call's count of the game's props or the mask's bits differ from
    /// the first call's.
    #[error(
        "crafting prop {prop}: the game's props and the mask read {first} and {width}, not {known_first} and {known_width} as before"
    )]
    Bounds {
        prop: Guid,
        first: u8,
        width: u8,
        known_first: u8,
        known_width: u8,
    },
    /// Every bit of the mask is taken.
    #[error(
        "crafting prop {prop}: the game's {width}-bit crafting prop mask is full, {first} by the game and {added} by mods"
    )]
    Full {
        prop: Guid,
        first: u8,
        width: u8,
        added: usize,
    },
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
    ) -> Result<(), DeclareIdError> {
        match self.known.get(&(space, key)) {
            Some((known, _)) if **known == *name => Ok(()),
            Some((known, by)) => Err(DeclareIdError::Taken {
                space,
                name: name.to_owned(),
                owner: owner.to_owned(),
                key,
                known: known.to_string(),
                by: by.to_string(),
            }),
            None => {
                self.known.insert((space, key), (name.into(), owner.into()));
                Ok(())
            }
        }
    }

    /// The index of the crafting prop `prop` in a mask of `width` bits
    /// whose first `first` the game's own props take: the next free one, or
    /// the one `prop` was given before.
    pub(crate) fn crafting_prop(
        &mut self,
        prop: Guid,
        first: u8,
        width: u8,
    ) -> Result<u8, CraftingPropError> {
        let props = &mut self.props;
        let (known_first, known_width) = *props.bounds.get_or_insert((first, width));
        if (first, width) != (known_first, known_width) {
            return Err(CraftingPropError::Bounds {
                prop,
                first,
                width,
                known_first,
                known_width,
            });
        }
        let at = props
            .added
            .iter()
            .position(|added| *added == prop)
            .unwrap_or(props.added.len());
        let index = u8::try_from(at)
            .ok()
            .and_then(|at| first.checked_add(at))
            .filter(|index| *index < width)
            .ok_or(CraftingPropError::Full {
                prop,
                first,
                width,
                added: props.added.len(),
            })?;
        if at == props.added.len() {
            props.added.push(prop);
        }
        Ok(index)
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

/// Declares content id `name` for the mod `owner` and returns its GUID:
/// refused for a malformed id, one outside the mod's namespace, or one whose
/// game id another content id has.
pub(crate) fn declare_content(owner: &str, name: &str) -> Result<Guid, DeclareIdError> {
    let guid = Guid::try_named(name).ok_or_else(|| DeclareIdError::Malformed {
        name: name.to_owned(),
    })?;
    if name
        .split_once(':')
        .is_none_or(|(namespace, _)| namespace != owner)
    {
        return Err(DeclareIdError::Outside {
            name: name.to_owned(),
            owner: owner.to_owned(),
        });
    }
    with(|ids| ids.declare(owner, Space::Content, name, guid.fnv1a()))?;
    Ok(guid)
}

/// The content id a mod declared whose GUID is exactly `guid`, not only its
/// game id, as a text that lives for the process: ids are never forgotten,
/// and a boxed name's heap text never moves.
pub(crate) fn content_named(guid: Guid) -> Option<enshwick_core::abi::Str> {
    with(|ids| declared(ids, guid).map(enshwick_core::abi::Str::new))
}

/// The content id a mod declared for exactly `guid`, copied.
pub(crate) fn content_id(guid: Guid) -> Option<String> {
    with(|ids| declared(ids, guid).map(str::to_owned))
}

/// The name in `ids` declared for exactly `guid`, not only for its key.
fn declared(ids: &Ids, guid: Guid) -> Option<&str> {
    ids.known
        .get(&(Space::Content, guid.fnv1a()))
        .map(|(name, _)| &**name)
        .filter(|name| Guid::try_named(name) == Some(guid))
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
        let why = ids
            .declare("b", Space::Content, "b:y", 7)
            .unwrap_err()
            .to_string();
        assert!(
            why.contains("b:y of b") && why.contains("a:x of a"),
            "{why}"
        );
    }

    #[test]
    fn crafting_props_take_the_bits_after_the_game_s_until_the_mask_is_full() {
        let mut ids = Ids::default();
        let prop = |n: u8| Guid([n; 16]);
        assert_eq!(ids.crafting_prop(prop(1), 62, 64), Ok(62));
        assert_eq!(ids.crafting_prop(prop(2), 62, 64), Ok(63));
        assert_eq!(ids.crafting_prop(prop(1), 62, 64), Ok(62), "given before");
        let full = ids.crafting_prop(prop(3), 62, 64).unwrap_err();
        assert!(full.to_string().contains("64-bit"), "{full}");
        assert!(matches!(
            ids.crafting_prop(prop(4), 51, 64),
            Err(CraftingPropError::Bounds { .. })
        ));
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
            declare_content("ids-test", "ids-test:lamp").ok(),
            Some(Guid::named("ids-test:lamp"))
        );
        assert!(declare_content("ids-test", "other:lamp").is_err());
        assert!(declare_content("ids-test", "lamp").is_err());
    }

    #[test]
    fn a_content_id_is_found_by_its_exact_guid_and_never_by_a_shared_key() {
        let locker = declare_content("ids-named", "ids-named:locker").unwrap();
        // SAFETY: the name lives in the ids for the process.
        let named = content_named(locker).map(|s| unsafe { s.as_str() }.unwrap().to_owned());
        assert_eq!(named.as_deref(), Some("ids-named:locker"));
        let crafted = Guid([3; 16]);
        with(|ids| {
            ids.known.insert(
                (Space::Content, crafted.fnv1a()),
                ("ids-named:other".into(), "ids-named".into()),
            )
        });
        assert!(
            content_named(crafted).is_none(),
            "the key's name is not this GUID"
        );
        assert!(content_named(Guid([4; 16])).is_none());
    }
}
