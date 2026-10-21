//! The game's own texts, read from its archive: the text collection names a
//! table per language by content hash, and each table lists its tags sorted
//! by id. The loader reads a language's table the first time a text in it
//! is asked for and keeps it for the process, so the texts it hands out
//! stay valid. Only the client's archive has them.
//!
//! A text missing in its language falls back to the game's English, then to
//! the text the developers wrote (`keenglish`), as the game's own lookup
//! does.

use std::sync::OnceLock;

/// The game's English (United States), the fallback of every language.
const ENGLISH: u32 = 1;
/// The most languages the game numbers, with room to grow.
const LANGUAGES: usize = 64;

/// Where the fields of the text resources sit, from the game's reflection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct TextLayout {
    /// `LocaTagCollectionResource.keenglishData`.
    pub keenglish: usize,
    /// `LocaTagCollectionResource.languages`.
    pub languages: usize,
    /// `LocaTagCollectionResourceLanguage` size.
    pub language_size: usize,
    /// `LocaTagCollectionResourceLanguage.language`.
    pub language_id: usize,
    /// `LocaTagCollectionResourceLanguage.data`.
    pub language_data: usize,
    /// `LocaTagCollectionResourceData.tags`.
    pub tags: usize,
    /// `LocaTagResource` size.
    pub tag_size: usize,
    /// `LocaTagResource.id`.
    pub tag_id: usize,
    /// `LocaTagResource.text`.
    pub tag_text: usize,
}

/// The text collection: the content hash of each language's table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Collection {
    keenglish: [u32; 4],
    languages: Vec<(u32, [u32; 4])>,
}

fn word(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn hash(bytes: &[u8], at: usize) -> Option<[u32; 4]> {
    Some([
        word(bytes, at)?,
        word(bytes, at + 4)?,
        word(bytes, at + 8)?,
        word(bytes, at + 12)?,
    ])
}

/// A blob array at `field`: its first element and count.
fn array(bytes: &[u8], field: usize) -> Option<(usize, usize)> {
    let offset = usize::try_from(word(bytes, field)?).ok()?;
    let count = usize::try_from(word(bytes, field + 4)?).ok()?;
    Some((field.checked_add(offset)?, count))
}

impl Collection {
    /// The collection in `resource`, the bytes of the game's
    /// `LocaTagCollectionResource`.
    pub(crate) fn parse(layout: &TextLayout, resource: &[u8]) -> Option<Self> {
        let keenglish = hash(resource, layout.keenglish)?;
        let (first, count) = array(resource, layout.languages)?;
        let languages = (0..count)
            .map(|i| {
                let at = first.checked_add(i.checked_mul(layout.language_size)?)?;
                Some((
                    word(resource, at + layout.language_id)?,
                    hash(resource, at + layout.language_data)?,
                ))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(Self {
            keenglish,
            languages,
        })
    }

    /// The table of `language`, the game's numbering; 0 is the text the
    /// developers wrote.
    fn table_of(&self, language: u32) -> Option<[u32; 4]> {
        if language == 0 {
            return Some(self.keenglish);
        }
        self.languages
            .iter()
            .find(|(id, _)| *id == language)
            .map(|(_, hash)| *hash)
    }
}

/// The text of tag `id` in `table`, the bytes of a
/// `LocaTagCollectionResourceData`.
pub(crate) fn find<'a>(layout: &TextLayout, table: &'a [u8], id: u32) -> Option<&'a str> {
    let (first, count) = array(table, layout.tags)?;
    let tag_at = |index: usize| first.checked_add(index.checked_mul(layout.tag_size)?);
    let (mut low, mut high) = (0usize, count);
    while low < high {
        let middle = low + (high - low) / 2;
        let at = tag_at(middle)?;
        let found = word(table, at + layout.tag_id)?;
        match found.cmp(&id) {
            core::cmp::Ordering::Less => low = middle + 1,
            core::cmp::Ordering::Greater => high = middle,
            core::cmp::Ordering::Equal => {
                let (text, len) = array(table, at + layout.tag_text)?;
                let bytes = table.get(text..text.checked_add(len)?)?;
                return core::str::from_utf8(bytes).ok();
            }
        }
    }
    None
}

/// The languages to look in for `language`, in order; 0 asks for English.
fn order(language: u32) -> [u32; 3] {
    if language == 0 {
        [ENGLISH, 0, 0]
    } else {
        [language, ENGLISH, 0]
    }
}

/// A language's table, read on first use; `None` when it does not read.
type Table = OnceLock<Option<Box<[u8]>>>;

/// The game's texts: the collection, and each language's table once read.
struct Texts {
    layout: TextLayout,
    collection: Collection,
    tables: Box<[Table]>,
}

impl Texts {
    fn table(
        &self,
        language: u32,
        read: impl FnOnce([u32; 4]) -> Option<Vec<u8>>,
    ) -> Option<&[u8]> {
        let slot = self.tables.get(usize::try_from(language).ok()?)?;
        slot.get_or_init(|| {
            let hash = self.collection.table_of(language)?;
            read(hash).map(Vec::into_boxed_slice)
        })
        .as_deref()
    }

    fn text(
        &self,
        id: u32,
        language: u32,
        read: impl Fn([u32; 4]) -> Option<Vec<u8>>,
    ) -> Option<&str> {
        order(language).into_iter().find_map(|language| {
            let table = self.table(language, &read)?;
            find(&self.layout, table, id)
        })
    }
}

impl Texts {
    fn new(layout: TextLayout, collection: Collection) -> Self {
        Self {
            layout,
            collection,
            tables: (0..LANGUAGES).map(|_| OnceLock::new()).collect(),
        }
    }
}

/// The game's text collection resource.
#[cfg(windows)]
const COLLECTION: enshwick_loader_api::abi::ResourceKey = enshwick_loader_api::abi::ResourceKey {
    guid: enshwick_loader_api::content::Guid::parse("163bb0a2-dc73-428b-b01a-a11e2eff947c").0,
    type_hash: 0x3f13_ff35,
    part: 0,
};

/// The game's texts, read on first use; `None` without the archive, its
/// collection (on a server) or the reflected layout.
#[cfg(windows)]
fn texts() -> Option<&'static Texts> {
    static TEXTS: OnceLock<Option<Texts>> = OnceLock::new();
    TEXTS
        .get_or_init(|| {
            let loader = crate::host::loader()?;
            let field = |ty: &str, name: &str| {
                let offset = loader.reflection.field_offset(ty, name).ok()?;
                usize::try_from(offset).ok()
            };
            let size = |ty: &str| {
                let info = loader.reflection.type_info(ty).ok()?;
                usize::try_from(info.size()).ok()
            };
            let layout = TextLayout {
                keenglish: field("keen::LocaTagCollectionResource", "keenglishData")?,
                languages: field("keen::LocaTagCollectionResource", "languages")?,
                language_size: size("keen::LocaTagCollectionResourceLanguage")?,
                language_id: field("keen::LocaTagCollectionResourceLanguage", "language")?,
                language_data: field("keen::LocaTagCollectionResourceLanguage", "data")?,
                tags: field("keen::LocaTagCollectionResourceData", "tags")?,
                tag_size: size("keen::LocaTagResource")?,
                tag_id: field("keen::LocaTagResource", "id")?,
                tag_text: field("keen::LocaTagResource", "text")?,
            };
            let resource = crate::archive::game()?.resource(&COLLECTION)?;
            let collection = Collection::parse(&layout, &resource)?;
            Some(Texts::new(layout, collection))
        })
        .as_ref()
}

/// The game's text of tag `id` in `language`, the game's numbering (0 for
/// English); `None` on a server or for a tag the game lacks.
#[cfg(windows)]
pub(crate) fn text(id: u32, language: u32) -> Option<&'static str> {
    let archive = crate::archive::game()?;
    texts()?.text(id, language, |hash| archive.content(&hash))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The layout of build 1269579.
    const LAYOUT: TextLayout = TextLayout {
        keenglish: 0,
        languages: 0x20,
        language_size: 0x28,
        language_id: 0,
        language_data: 8,
        tags: 0,
        tag_size: 0x18,
        tag_id: 0,
        tag_text: 4,
    };

    /// A table with `tags`, sorted by id as the game keeps them.
    fn table(tags: &[(u32, &str)]) -> Vec<u8> {
        let mut bytes = vec![0u8; 8 + tags.len() * 0x18];
        bytes[..4].copy_from_slice(&8u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&u32::try_from(tags.len()).unwrap().to_le_bytes());
        for (i, (id, text)) in tags.iter().enumerate() {
            let at = 8 + i * 0x18;
            let text_at = bytes.len();
            bytes.extend_from_slice(text.as_bytes());
            bytes[at..at + 4].copy_from_slice(&id.to_le_bytes());
            let offset = u32::try_from(text_at - (at + 4)).unwrap();
            bytes[at + 4..at + 8].copy_from_slice(&offset.to_le_bytes());
            let len = u32::try_from(text.len()).unwrap();
            bytes[at + 8..at + 12].copy_from_slice(&len.to_le_bytes());
        }
        bytes
    }

    fn collection() -> Vec<u8> {
        let mut bytes = vec![0u8; 0x28 + 2 * 0x28];
        bytes[..4].copy_from_slice(&100u32.to_le_bytes());
        bytes[0x20..0x24].copy_from_slice(&8u32.to_le_bytes());
        bytes[0x24..0x28].copy_from_slice(&2u32.to_le_bytes());
        for (i, (language, size)) in [(1u32, 101u32), (34, 134)].into_iter().enumerate() {
            let at = 0x28 + i * 0x28;
            bytes[at..at + 4].copy_from_slice(&language.to_le_bytes());
            bytes[at + 8..at + 12].copy_from_slice(&size.to_le_bytes());
        }
        bytes
    }

    #[test]
    fn a_tag_is_found_by_its_id() {
        let table = table(&[(3, "three"), (7, "seven"), (9, "nine")]);
        assert_eq!(find(&LAYOUT, &table, 7), Some("seven"));
        assert_eq!(find(&LAYOUT, &table, 3), Some("three"));
        assert_eq!(find(&LAYOUT, &table, 9), Some("nine"));
        assert_eq!(find(&LAYOUT, &table, 8), None);
        assert_eq!(find(&LAYOUT, &table[..20], 9), None);
    }

    #[test]
    fn a_missing_text_falls_back_to_english_then_keenglish() {
        let collection = Collection::parse(&LAYOUT, &collection()).unwrap();
        assert_eq!(collection.table_of(34).map(|h| h[0]), Some(134));
        assert_eq!(collection.table_of(0).map(|h| h[0]), Some(100));
        assert_eq!(collection.table_of(3), None);
        let texts = Texts::new(LAYOUT, collection);
        let read = |hash: [u32; 4]| match hash[0] {
            100 => Some(table(&[(1, "dev"), (2, "dev two"), (3, "dev three")])),
            101 => Some(table(&[(1, "Balthazar"), (2, "Two")])),
            134 => Some(table(&[(1, "Бальтазар")])),
            _ => None,
        };
        assert_eq!(texts.text(1, 34, read), Some("Бальтазар"));
        assert_eq!(texts.text(2, 34, read), Some("Two"));
        assert_eq!(texts.text(3, 34, read), Some("dev three"));
        assert_eq!(texts.text(1, 3, read), Some("Balthazar"));
        assert_eq!(texts.text(1, 0, read), Some("Balthazar"));
        assert_eq!(texts.text(3, 0, read), Some("dev three"));
        assert_eq!(texts.text(4, 34, read), None);
        assert_eq!(texts.text(1, 99, read), Some("Balthazar"));
    }

    /// The installed client's archive, when `ENSHWICK_CLIENT_DIR` names its
    /// folder (build 1269579): the Alchemist's staff names him in English
    /// and in Ukrainian.
    #[test]
    fn the_client_archive_names_the_alchemist() {
        let Ok(dir) = std::env::var("ENSHWICK_CLIENT_DIR") else {
            return;
        };
        let archive =
            crate::archive::Archive::open(std::path::Path::new(&dir), "enshrouded").unwrap();
        let collection = enshwick_loader_api::abi::ResourceKey {
            guid: enshwick_loader_api::content::Guid::parse("163bb0a2-dc73-428b-b01a-a11e2eff947c")
                .0,
            type_hash: 0x3f13_ff35,
            part: 0,
        };
        let resource = archive.resource(&collection).unwrap();
        let texts = Texts::new(LAYOUT, Collection::parse(&LAYOUT, &resource).unwrap());
        let read = |hash: [u32; 4]| archive.content(&hash);
        assert_eq!(texts.text(0xa334_5716, 1, read), Some("Balthazar"));
        assert_eq!(texts.text(0xa334_5716, 34, read), Some("Бальтазар"));
    }
}
