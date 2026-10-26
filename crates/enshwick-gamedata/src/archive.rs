//! The game's own archive, read from its files: resources by key and
//! contents by hash, for mods and for the loader's lookups of vanilla names
//! and texts, free of the game's state and threads.
//!
//! `<name>.kfc` holds the maps (format `KFC4`, game build 1269579). Resources
//! sit in `<name>.kfc_resources`, compressed with zstd in chunks the header
//! lists by their packed and unpacked ranges; a resource's offset and size
//! count in the unpacked bytes. Contents sit uncompressed in
//! `<name>_<container>.dat`.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use enshwick::abi::ResourceKey;
use rustc_hash::FxHashMap;

/// A content hash as the game keeps it: the size, then three words.
pub type ContentHash = [u32; 4];

/// The header's magic, `KFC4`.
const MAGIC: u32 = 0x3443_464b;
/// Header fields: each a location, an offset from the field and a count.
const CONTENT_BUCKETS: usize = 0x40;
const CONTENT_KEYS: usize = 0x48;
const CONTENT_VALUES: usize = 0x50;
const RESOURCE_KEYS: usize = 0x60;
const RESOURCE_VALUES: usize = 0x68;
const CHUNKS: usize = 0x88;
/// A resource key in the map: GUID, type hash, part and 8 reserved bytes.
const RESOURCE_KEY: usize = 32;
/// A chunk: packed offset, packed room, packed length, unpacked offset and
/// unpacked length.
const CHUNK: usize = 20;
/// Unpacked chunks kept for the next read.
const CACHED: usize = 4;
/// The zstd frame magic; a chunk without it is stored as is.
const ZSTD: [u8; 4] = [0x28, 0xb5, 0x2f, 0xfd];

/// A resource's place in the unpacked resource bytes.
#[derive(Clone, Copy, Debug)]
struct Place {
    at: u32,
    len: u32,
}

#[derive(Clone, Copy, Debug)]
struct Chunk {
    packed_at: u32,
    packed_len: u32,
    at: u32,
    len: u32,
}

/// One archive of the game, opened.
#[derive(Debug)]
pub struct Archive {
    dir: PathBuf,
    name: String,
    header: Box<[u8]>,
    resources: FxHashMap<([u8; 16], u32, u32), Place>,
    chunks: Box<[Chunk]>,
    cache: Mutex<Vec<(usize, Arc<[u8]>)>>,
}

fn word(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

/// A buffer of `len` zero bytes; `None` when the memory is not there, so a
/// size read from a damaged file fails the read instead of the game.
fn zeroed(len: usize) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(len).ok()?;
    bytes.resize(len, 0);
    Some(bytes)
}

/// Whether `count` entries of `size` bytes from `at` lie inside `bytes`.
fn holds(bytes: &[u8], at: usize, count: usize, size: usize) -> bool {
    count
        .checked_mul(size)
        .and_then(|len| at.checked_add(len))
        .is_some_and(|end| end <= bytes.len())
}

/// Opens `path` and moves to `at`, when `len` bytes from there lie inside
/// the file.
fn open_at(path: &Path, at: u32, len: usize) -> Option<File> {
    let mut file = File::open(path).ok()?;
    let end = u64::from(at).checked_add(u64::try_from(len).ok()?)?;
    if end > file.metadata().ok()?.len() {
        return None;
    }
    file.seek(SeekFrom::Start(u64::from(at))).ok()?;
    Some(file)
}

/// A location in the header: where it points and its count.
fn location(header: &[u8], field: usize) -> Option<(usize, usize)> {
    let offset = usize::try_from(word(header, field)?).ok()?;
    let count = usize::try_from(word(header, field + 4)?).ok()?;
    Some((field.checked_add(offset)?, count))
}

impl Archive {
    /// The archive `<dir>/<name>.kfc`; `None` when it is missing or in a
    /// format the loader does not read.
    #[must_use]
    pub fn open(dir: &Path, name: &str) -> Option<Self> {
        let header = std::fs::read(dir.join(format!("{name}.kfc"))).ok()?;
        Self::from_header(dir, name, header.into_boxed_slice())
    }

    fn from_header(dir: &Path, name: &str, header: Box<[u8]>) -> Option<Self> {
        if word(&header, 0)? != MAGIC {
            return None;
        }
        let (keys, count) = location(&header, RESOURCE_KEYS)?;
        let (values, values_count) = location(&header, RESOURCE_VALUES)?;
        if values_count != count
            || !holds(&header, keys, count, RESOURCE_KEY)
            || !holds(&header, values, count, 8)
        {
            return None;
        }
        let mut resources = FxHashMap::default();
        resources.reserve(count);
        for i in 0..count {
            let key = keys.checked_add(i.checked_mul(RESOURCE_KEY)?)?;
            let guid: [u8; 16] = header.get(key..key + 16)?.try_into().ok()?;
            let type_hash = word(&header, key + 16)?;
            let part = word(&header, key + 20)?;
            let value = values.checked_add(i.checked_mul(8)?)?;
            let place = Place {
                at: word(&header, value)?,
                len: word(&header, value + 4)?,
            };
            resources.insert((guid, type_hash, part), place);
        }
        let (chunks_at, chunk_count) = location(&header, CHUNKS)?;
        if !holds(&header, chunks_at, chunk_count, CHUNK) {
            return None;
        }
        let chunks = (0..chunk_count)
            .map(|i| {
                let at = chunks_at.checked_add(i.checked_mul(CHUNK)?)?;
                Some(Chunk {
                    packed_at: word(&header, at)?,
                    packed_len: word(&header, at + 8)?,
                    at: word(&header, at + 12)?,
                    len: word(&header, at + 16)?,
                })
            })
            .collect::<Option<Box<[Chunk]>>>()?;
        Some(Self {
            dir: dir.to_owned(),
            name: name.to_owned(),
            header,
            resources,
            chunks,
            cache: Mutex::new(Vec::new()),
        })
    }

    /// The bytes of content `hash`; `None` for a hash the archive lacks or
    /// keeps compressed.
    #[must_use]
    pub fn content(&self, hash: &ContentHash) -> Option<Vec<u8>> {
        let (container, offset) = find_content(&self.header, hash)?;
        let path = self.dir.join(format!("{}_{container:03}.dat", self.name));
        let len = usize::try_from(hash[0]).ok()?;
        let mut file = open_at(&path, offset, len)?;
        let mut data = zeroed(len)?;
        file.read_exact(&mut data).ok()?;
        Some(data)
    }

    /// The bytes of resource `key`, as the game reads them before it
    /// resolves their references; `None` for a key the archive lacks.
    #[must_use]
    pub fn resource(&self, key: &ResourceKey) -> Option<Vec<u8>> {
        let place = *self.resources.get(&(key.guid, key.type_hash, key.part))?;
        let (start, len) = (place.at, place.len);
        let end = start.checked_add(len)?;
        let mut out = Vec::new();
        out.try_reserve_exact(usize::try_from(len).ok()?).ok()?;
        for (index, chunk) in self.chunks.iter().enumerate() {
            let chunk_end = chunk.at.saturating_add(chunk.len);
            if chunk_end <= start || chunk.at >= end {
                continue;
            }
            let bytes = self.chunk(index)?;
            let from = usize::try_from(start.max(chunk.at) - chunk.at).ok()?;
            let to = usize::try_from(end.min(chunk_end) - chunk.at).ok()?;
            out.extend_from_slice(bytes.get(from..to)?);
        }
        (out.len() == usize::try_from(len).ok()?).then_some(out)
    }

    /// The keys of the resources of type `type_hash`, the fnv1a32 hash of
    /// the type's qualified name, for a type the game keeps one resource of.
    #[must_use]
    pub fn keys_of(&self, type_hash: u32) -> Vec<ResourceKey> {
        self.resources
            .keys()
            .filter(|(_, ty, _)| *ty == type_hash)
            .map(|&(guid, type_hash, part)| ResourceKey {
                guid,
                type_hash,
                part,
            })
            .collect()
    }

    /// Unpacked chunk `index`, from the cache or the file.
    fn chunk(&self, index: usize) -> Option<Arc<[u8]>> {
        if let Some((_, bytes)) = self
            .cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(cached, _)| *cached == index)
        {
            return Some(Arc::clone(bytes));
        }
        let chunk = self.chunks.get(index)?;
        let path = self.dir.join(format!("{}.kfc_resources", self.name));
        let packed_len = usize::try_from(chunk.packed_len).ok()?;
        let mut file = open_at(&path, chunk.packed_at, packed_len)?;
        let mut packed = zeroed(packed_len)?;
        file.read_exact(&mut packed).ok()?;
        let len = usize::try_from(chunk.len).ok()?;
        let bytes: Arc<[u8]> = if packed.starts_with(&ZSTD) {
            let mut unpacked = zeroed(len)?;
            let written = ruzstd::decoding::FrameDecoder::new()
                .decode_all(&packed, &mut unpacked)
                .ok()?;
            (written == len).then_some(unpacked)?.into()
        } else if packed.len() == len {
            packed.into()
        } else {
            return None;
        };
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        if cache.len() >= CACHED {
            cache.remove(0);
        }
        cache.push((index, Arc::clone(&bytes)));
        Some(bytes)
    }

    /// Lets go of the unpacked chunks kept for later reads.
    pub fn trim(&self) {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clear();
    }
}

/// The container and offset of an uncompressed content `hash`.
fn find_content(header: &[u8], hash: &ContentHash) -> Option<(u16, u32)> {
    if word(header, 0)? != MAGIC {
        return None;
    }
    let (buckets, bucket_count) = location(header, CONTENT_BUCKETS)?;
    let (keys, _) = location(header, CONTENT_KEYS)?;
    let (values, _) = location(header, CONTENT_VALUES)?;
    if bucket_count == 0 || !bucket_count.is_power_of_two() {
        return None;
    }
    let bucket = buckets + (usize::try_from(hash[1]).ok()? & (bucket_count - 1)) * 8;
    let first = usize::try_from(word(header, bucket)?).ok()?;
    let count = usize::try_from(word(header, bucket + 4)?).ok()?;
    let held = header.len().saturating_sub(keys) / 16;
    for index in first..first.checked_add(count)?.min(held) {
        let key = keys + index * 16;
        if (0..4).all(|i| word(header, key + i * 4) == Some(hash[i])) {
            let value = values + index * 16;
            let offset = word(header, value)?;
            let flags = u16::from_le_bytes(header.get(value + 4..value + 6)?.try_into().ok()?);
            let container = u16::from_le_bytes(header.get(value + 6..value + 8)?.try_into().ok()?);
            return (flags == 0).then_some((container, offset));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes `value` at `at`.
    fn put(h: &mut [u8], at: usize, value: u32) {
        h[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    /// Points header `field` at `at` with `count` entries.
    fn locate(h: &mut [u8], field: usize, at: usize, count: u32) {
        put(h, field, u32::try_from(at - field).unwrap());
        put(h, field + 4, count);
    }

    /// An archive in a temporary folder: two contents (the second
    /// compressed) and two resources over two chunks, one packed with zstd
    /// and one stored.
    fn archive() -> (PathBuf, Archive) {
        let dir = std::env::temp_dir().join(format!(
            "enshwick-archive-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let mut h = vec![0u8; 0x200];
        put(&mut h, 0, MAGIC);
        locate(&mut h, CONTENT_BUCKETS, 0x100, 1);
        locate(&mut h, CONTENT_KEYS, 0x108, 2);
        locate(&mut h, CONTENT_VALUES, 0x128, 2);
        put(&mut h, 0x104, 2);
        for (i, key) in [[4u32, 1, 2, 3], [20, 4, 5, 6]].iter().enumerate() {
            for (j, w) in key.iter().enumerate() {
                put(&mut h, 0x108 + i * 16 + j * 4, *w);
            }
        }
        put(&mut h, 0x128, 2);
        h[0x13c..0x13e].copy_from_slice(&1u16.to_le_bytes());
        locate(&mut h, RESOURCE_KEYS, 0x150, 2);
        locate(&mut h, RESOURCE_VALUES, 0x190, 2);
        h[0x150..0x160].copy_from_slice(&[7; 16]);
        put(&mut h, 0x160, 0xabcd);
        h[0x170..0x180].copy_from_slice(&[8; 16]);
        put(&mut h, 0x180, 0xabcd);
        // The first resource runs across both chunks, the second sits in
        // the stored one.
        put(&mut h, 0x190, 6);
        put(&mut h, 0x194, 4);
        put(&mut h, 0x198, 10);
        put(&mut h, 0x19c, 3);
        let first: Vec<u8> = (0u8..8).collect();
        let second: Vec<u8> = (8u8..16).collect();
        let packed = ruzstd::encoding::compress_to_vec(
            first.as_slice(),
            ruzstd::encoding::CompressionLevel::Fastest,
        );
        locate(&mut h, CHUNKS, 0x1a0, 2);
        let packed_len = u32::try_from(packed.len()).unwrap();
        for (at, value) in [
            (0x1a0, 0),
            (0x1a4, packed_len),
            (0x1a8, packed_len),
            (0x1ac, 0),
        ]
        .into_iter()
        .chain([(0x1b0, 8)])
        {
            put(&mut h, at, value);
        }
        for (at, value) in [
            (0x1b4, packed_len),
            (0x1b8, 8),
            (0x1bc, 8),
            (0x1c0, 8),
            (0x1c4, 8),
        ] {
            put(&mut h, at, value);
        }
        std::fs::write(dir.join("game.kfc"), &h).unwrap();
        std::fs::write(
            dir.join("game.kfc_resources"),
            [packed.as_slice(), second.as_slice()].concat(),
        )
        .unwrap();
        std::fs::write(dir.join("game_000.dat"), [0, 0, 9, 8, 7, 6]).unwrap();
        let archive = Archive::open(&dir, "game").unwrap();
        (dir, archive)
    }

    #[test]
    fn contents_and_resources_read_across_chunks() {
        let (dir, archive) = archive();
        assert_eq!(archive.content(&[4, 1, 2, 3]), Some(vec![9, 8, 7, 6]));
        assert_eq!(archive.content(&[20, 4, 5, 6]), None, "compressed");
        assert_eq!(archive.content(&[4, 1, 2, 4]), None);
        let key = |guid, part| ResourceKey {
            guid: [guid; 16],
            type_hash: 0xabcd,
            part,
        };
        assert_eq!(archive.resource(&key(7, 0)), Some(vec![6, 7, 8, 9]));
        assert_eq!(archive.resource(&key(8, 0)), Some(vec![10, 11, 12]));
        assert_eq!(archive.resource(&key(8, 1)), None);
        archive.trim();
        assert_eq!(archive.resource(&key(7, 0)), Some(vec![6, 7, 8, 9]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn counts_and_sizes_past_the_file_fail_the_read() {
        let (dir, archive) = archive();
        let mut h = std::fs::read(dir.join("game.kfc")).unwrap();
        for field in [RESOURCE_KEYS, CHUNKS] {
            let mut broken = h.clone();
            put(&mut broken, field + 4, u32::MAX);
            if field == RESOURCE_KEYS {
                put(&mut broken, RESOURCE_VALUES + 4, u32::MAX);
            }
            assert!(Archive::from_header(&dir, "game", broken.into_boxed_slice()).is_none());
        }
        // A bucket that claims every entry stops at the header's end.
        put(&mut h, 0x104, u32::MAX);
        assert_eq!(find_content(&h, &[4, 1, 2, 4]), None);
        assert_eq!(find_content(&h, &[4, 1, 2, 3]), Some((0, 2)));
        // A content larger than its container is not read.
        assert_eq!(archive.content(&[4, 1, 2, 3]).map(|c| c.len()), Some(4));
        assert!(open_at(&dir.join("game_000.dat"), 2, 5).is_none());
        assert!(open_at(&dir.join("game_000.dat"), u32::MAX, usize::MAX).is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn another_format_does_not_open() {
        let mut header = vec![0u8; 0x100];
        header[..4].copy_from_slice(b"KFC2");
        assert!(Archive::from_header(Path::new("."), "x", header.into_boxed_slice()).is_none());
        assert!(Archive::from_header(Path::new("."), "x", Box::new([])).is_none());
    }

    /// The installed client's archive, when `ENSHWICK_CLIENT_DIR` names its
    /// folder (build 1269579): the Blacksmith's staff icon reads whole and
    /// decodes, and the item registry lists the Workbench.
    #[test]
    fn the_client_archive_reads() {
        let Ok(dir) = std::env::var("ENSHWICK_CLIENT_DIR") else {
            return;
        };
        let archive = Archive::open(Path::new(&dir), "enshrouded").unwrap();
        let icon = archive
            .content(&[0x1_5570, 0xfa5b_71ba, 0x1c35_6c97, 0x170c_1f07])
            .unwrap();
        assert_eq!(icon.len(), 0x1_5570);
        let mut rgba = enshwick::content::icons::decode(
            enshwick::content::icons::formats::BC7,
            256,
            256,
            &icon,
        )
        .unwrap();
        let opaque = rgba
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|p| p[3] > 200)
            .count();
        assert!(opaque > 256 * 256 / 2, "{opaque} opaque pixels");
        let tint = enshwick::content::icons::Tint {
            colour: [120, 170, 255],
            amount: 60,
        };
        enshwick::content::icons::tint(&mut rgba, tint);
        assert!(rgba.as_chunks::<4>().0.iter().any(|p| p[2] > p[0]), "bluer");
        let registry =
            enshwick::content::items::ITEM_REGISTRY.key(enshwick::content::types::ITEM_REGISTRY);
        let registry = archive.resource(&registry).unwrap();
        let workbench = enshwick::content::workshops::stations::WORKBENCH.item;
        assert!(registry.windows(16).any(|w| w == workbench.0));
    }
}
