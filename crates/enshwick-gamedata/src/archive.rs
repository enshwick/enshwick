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
use std::sync::{Arc, Mutex};

use enshwick_base::abi::ResourceKey;
use enshwick_base::content::Guid;
use rustc_hash::FxHashMap;

use enshwick_base::Relock;

/// A content hash as the game keeps it: the size, then three words.
pub type ContentHash = [u32; 4];

/// The header's magic, `KFC4`.
const MAGIC: u32 = 0x3443_464b;
/// The version string: an offset from the field and its length (client
/// archive 1283531, server archive 1273043).
const VERSION: usize = 0x10;
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
/// The most a zstd block unpacks to; each block takes at least three bytes
/// of header, which bounds what a packed chunk can unpack to.
const ZSTD_BLOCK: usize = 128 * 1024;
/// What the map keeps of a resource's length: its low 24 bits. On client
/// 26430625 and its server, `VoxelTemperatureResource`
/// dbea9f50604c5f429c7cdeeefd9b6920 part 3 holds 0x2a9748c bytes, its
/// volume header's own size, and the map stores 0xa9748c.
const CUT: u32 = 1 << 24;
/// What the archive aligns each resource's offset to: a resource ends fewer
/// than this many bytes before the next one's offset.
const ALIGN: u32 = 16;

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

/// Where an archive's resources and contents are read from.
#[derive(Debug)]
enum Store {
    /// `<name>.kfc_resources` and `<name>_<container>.dat` in `dir`.
    Files { dir: PathBuf, name: String },
    /// Bytes held in memory: the resources and each container.
    Bytes {
        resources: Box<[u8]>,
        containers: FxHashMap<u16, Box<[u8]>>,
    },
}

/// A part of an archive besides its header.
#[derive(Clone, Copy, Debug)]
enum Part {
    Resources,
    Container(u16),
}

impl Store {
    /// `len` bytes of `part` at `at`; `None` when they do not lie inside it.
    fn read(&self, part: Part, at: u32, len: usize) -> Option<Vec<u8>> {
        match self {
            Self::Files { dir, name } => {
                let path = match part {
                    Part::Resources => dir.join(format!("{name}.kfc_resources")),
                    Part::Container(container) => dir.join(format!("{name}_{container:03}.dat")),
                };
                let mut file = open_at(&path, at, len)?;
                let mut data = zeroed(len)?;
                file.read_exact(&mut data).ok()?;
                Some(data)
            }
            Self::Bytes {
                resources,
                containers,
            } => {
                let bytes = match part {
                    Part::Resources => resources,
                    Part::Container(container) => containers.get(&container)?,
                };
                let at = usize::try_from(at).ok()?;
                Some(bytes.get(at..at.checked_add(len)?)?.to_vec())
            }
        }
    }
}

/// One archive of the game, opened.
#[derive(Debug)]
pub struct Archive {
    store: Store,
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

/// `len`, a length the map stores, with the multiples of [`CUT`] it lost put
/// back where they fill `room`, the bytes from the resource's offset to the
/// next one's, to within its [`ALIGN`] padding; any other `len` stays, such
/// as that of a part sharing its offset with a longer one or followed by a
/// gap.
fn uncut(len: u32, room: u32) -> u32 {
    let Some(lost) = room.checked_sub(len) else {
        return len;
    };
    if len >= CUT || lost % CUT >= ALIGN {
        return len;
    }
    len + lost / CUT * CUT
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
        let store = Store::Files {
            dir: dir.to_owned(),
            name: name.to_owned(),
        };
        Self::from_header(store, header.into_boxed_slice())
    }

    /// An archive held in memory: the bytes of `<name>.kfc`, of
    /// `<name>.kfc_resources` and of each `<name>_<container>.dat`, for tests
    /// and tools that have the files' bytes rather than the files; `None` in
    /// a format the loader does not read.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::archive::Archive;
    ///
    /// assert!(Archive::from_bytes(b"KFC2".to_vec(), Vec::new(), []).is_none());
    /// ```
    #[must_use]
    pub fn from_bytes(
        header: Vec<u8>,
        resources: Vec<u8>,
        containers: impl IntoIterator<Item = (u16, Vec<u8>)>,
    ) -> Option<Self> {
        let store = Store::Bytes {
            resources: resources.into_boxed_slice(),
            containers: containers
                .into_iter()
                .map(|(container, bytes)| (container, bytes.into_boxed_slice()))
                .collect(),
        };
        Self::from_header(store, header.into_boxed_slice())
    }

    fn from_header(store: Store, header: Box<[u8]>) -> Option<Self> {
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
        let end = chunks
            .iter()
            .map(|c| c.at.saturating_add(c.len))
            .max()
            .unwrap_or(0);
        let mut starts: Vec<u32> = resources.values().map(|p| p.at).collect();
        starts.sort_unstable();
        starts.dedup();
        for place in resources.values_mut() {
            let next = starts
                .get(starts.partition_point(|&s| s <= place.at))
                .map_or(end, |&next| next);
            place.len = uncut(place.len, next.saturating_sub(place.at));
        }
        Some(Self {
            store,
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
        let len = usize::try_from(hash[0]).ok()?;
        self.store.read(Part::Container(container), offset, len)
    }

    /// Whether the archive holds resource `key`, without reading it.
    #[must_use]
    pub fn contains(&self, key: &ResourceKey) -> bool {
        self.resources
            .contains_key(&(key.guid.0, key.type_hash, key.part))
    }

    /// The whole length of resource `key`, given `len`, the length the
    /// game's own map gives it: the archive's length when `len` is that cut
    /// to its low 24 bits, as the map stores it, else `len`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::archive::Archive;
    ///
    /// if let Some(archive) = Archive::open(std::path::Path::new("game"), "enshrouded")
    ///     && let Some(key) = archive.keys().first()
    ///     && let Some(bytes) = archive.resource(key)
    /// {
    ///     assert_eq!(archive.uncut_len(key, bytes.len()), bytes.len());
    /// }
    /// ```
    #[must_use]
    pub fn uncut_len(&self, key: &ResourceKey, len: usize) -> usize {
        let whole = || {
            let whole = self
                .resources
                .get(&(key.guid.0, key.type_hash, key.part))?
                .len;
            let cut = u32::try_from(len).ok()?;
            (whole > cut && whole % CUT == cut).then_some(usize::try_from(whole).ok()?)
        };
        whole().unwrap_or(len)
    }

    /// The bytes of resource `key`, as the game reads them before it
    /// resolves their references, whole even where the map stores its
    /// length cut; `None` for a key the archive lacks.
    #[must_use]
    pub fn resource(&self, key: &ResourceKey) -> Option<Vec<u8>> {
        let place = *self.resources.get(&(key.guid.0, key.type_hash, key.part))?;
        let (start, len) = (place.at, place.len);
        let end = start.checked_add(len)?;
        // The chunks must cover the whole resource.
        let covered: u64 = self
            .chunks
            .iter()
            .map(|c| {
                let from = start.max(c.at);
                let to = end.min(c.at.saturating_add(c.len));
                u64::from(to.saturating_sub(from))
            })
            .sum();
        if covered != u64::from(len) {
            return None;
        }
        // The bytes grow as the chunks unpack, never by the size the map
        // claims, so a damaged size costs no memory.
        let mut out = Vec::new();
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
    /// the type's qualified name, in the archive's order.
    #[must_use]
    pub fn keys_of(&self, type_hash: u32) -> Vec<ResourceKey> {
        let mut keys: Vec<_> = self
            .resources
            .iter()
            .filter(|((_, ty, _), _)| *ty == type_hash)
            .collect();
        keys.sort_unstable_by_key(|(key, place)| (place.at, **key));
        keys.into_iter()
            .map(|(&(guid, type_hash, part), _)| ResourceKey {
                guid: Guid(guid),
                type_hash,
                part,
            })
            .collect()
    }

    /// The archive's version string, such as
    /// `1283531|^/game38/branches/release_1.0|2026-10-30T09:15:50.868196Z` on
    /// client archive 1283531: the build number, the branch and when it was
    /// built. `None` when the header holds no UTF-8 string there.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::archive::Archive;
    ///
    /// if let Some(archive) = Archive::open(std::path::Path::new("game"), "enshrouded") {
    ///     assert!(archive.version().is_some_and(|v| v.contains('|')));
    /// }
    /// ```
    #[must_use]
    pub fn version(&self) -> Option<&str> {
        let (at, len) = location(&self.header, VERSION)?;
        std::str::from_utf8(self.header.get(at..at.checked_add(len)?)?).ok()
    }

    /// Every resource key the archive holds, in the order their bytes lie,
    /// so reading them in turn unpacks each chunk once.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::archive::Archive;
    ///
    /// if let Some(archive) = Archive::open(std::path::Path::new("game"), "enshrouded") {
    ///     assert!(!archive.keys().is_empty());
    /// }
    /// ```
    #[must_use]
    pub fn keys(&self) -> Vec<ResourceKey> {
        let mut keys: Vec<_> = self.resources.iter().collect();
        keys.sort_unstable_by_key(|(key, place)| (place.at, **key));
        keys.into_iter()
            .map(|(&(guid, type_hash, part), _)| ResourceKey {
                guid: Guid(guid),
                type_hash,
                part,
            })
            .collect()
    }

    /// Every content hash the archive holds, in the archive's order.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_gamedata::archive::Archive;
    ///
    /// if let Some(archive) = Archive::open(std::path::Path::new("game"), "enshrouded") {
    ///     let first = archive.contents().into_iter().next();
    ///     assert!(first.is_none_or(|hash| archive.content(&hash).is_some()));
    /// }
    /// ```
    #[must_use]
    pub fn contents(&self) -> Vec<ContentHash> {
        let Some((keys, count)) = location(&self.header, CONTENT_KEYS) else {
            return Vec::new();
        };
        let held = self.header.len().saturating_sub(keys) / 16;
        (0..count.min(held))
            .filter_map(|i| {
                let key = keys + i * 16;
                Some([
                    word(&self.header, key)?,
                    word(&self.header, key + 4)?,
                    word(&self.header, key + 8)?,
                    word(&self.header, key + 12)?,
                ])
            })
            .collect()
    }

    /// Unpacked chunk `index`, from the cache or the file.
    fn chunk(&self, index: usize) -> Option<Arc<[u8]>> {
        if let Some((_, bytes)) = self
            .cache
            .relock()
            .iter()
            .find(|(cached, _)| *cached == index)
        {
            return Some(Arc::clone(bytes));
        }
        let chunk = self.chunks.get(index)?;
        let packed_len = usize::try_from(chunk.packed_len).ok()?;
        let packed = self
            .store
            .read(Part::Resources, chunk.packed_at, packed_len)?;
        let len = usize::try_from(chunk.len).ok()?;
        let bytes: Arc<[u8]> = if packed.starts_with(&ZSTD) {
            if len > packed_len.div_ceil(3).saturating_mul(ZSTD_BLOCK) {
                return None;
            }
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
        let mut cache = self.cache.relock();
        if cache.len() >= CACHED {
            cache.remove(0);
        }
        cache.push((index, Arc::clone(&bytes)));
        Some(bytes)
    }

    /// Lets go of the unpacked chunks kept for later reads.
    pub fn trim(&self) {
        self.cache.relock().clear();
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

    /// The bytes of an archive's header, resources and container 0: two
    /// contents (the second compressed) and two resources over two chunks,
    /// one packed with zstd and one stored.
    fn parts() -> (Vec<u8>, Vec<u8>, Vec<u8>) {
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
        let resources = [packed.as_slice(), second.as_slice()].concat();
        (h, resources, vec![0, 0, 9, 8, 7, 6])
    }

    /// The archive of [`parts`] in a temporary folder.
    fn archive() -> (PathBuf, Archive) {
        let dir = std::env::temp_dir().join(format!(
            "enshwick-archive-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let (header, resources, container) = parts();
        std::fs::write(dir.join("game.kfc"), header).unwrap();
        std::fs::write(dir.join("game.kfc_resources"), resources).unwrap();
        std::fs::write(dir.join("game_000.dat"), container).unwrap();
        let archive = Archive::open(&dir, "game").unwrap();
        (dir, archive)
    }

    fn key(guid: u8, part: u32) -> ResourceKey {
        ResourceKey {
            guid: Guid([guid; 16]),
            type_hash: 0xabcd,
            part,
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "unpacking archive chunks takes minutes under Miri")]
    fn an_archive_in_memory_reads_as_its_files_do() {
        let (header, resources, container) = parts();
        let archive = Archive::from_bytes(header, resources, [(0, container)]).unwrap();
        assert_eq!(archive.content(&[4, 1, 2, 3]), Some(vec![9, 8, 7, 6]));
        assert_eq!(archive.content(&[20, 4, 5, 6]), None, "compressed");
        assert_eq!(archive.resource(&key(7, 0)), Some(vec![6, 7, 8, 9]));
        assert_eq!(archive.resource(&key(8, 0)), Some(vec![10, 11, 12]));
        let (header, resources, _) = parts();
        let without = Archive::from_bytes(header, resources, []).unwrap();
        assert_eq!(without.content(&[4, 1, 2, 3]), None, "no container 0");
    }

    #[test]
    #[cfg_attr(miri, ignore = "unpacking archive chunks takes minutes under Miri")]
    fn sizes_the_chunks_cannot_hold_take_no_memory() {
        let (mut header, resources, container) = parts();
        // The first resource claims nearly 4 GiB from its offset.
        put(&mut header, 0x194, u32::MAX - 6);
        // The zstd chunk claims to unpack to 4 GiB.
        let mut huge = header.clone();
        put(&mut huge, 0x1b0, u32::MAX);
        let archive =
            Archive::from_bytes(header, resources.clone(), [(0, container.clone())]).unwrap();
        assert_eq!(archive.resource(&key(7, 0)), None);
        let archive = Archive::from_bytes(huge, resources, [(0, container)]).unwrap();
        assert_eq!(archive.chunk(0), None);
    }

    #[test]
    #[cfg_attr(miri, ignore = "unpacking archive chunks takes minutes under Miri")]
    fn contents_and_resources_read_across_chunks() {
        let (dir, archive) = archive();
        assert_eq!(archive.content(&[4, 1, 2, 3]), Some(vec![9, 8, 7, 6]));
        assert_eq!(archive.content(&[20, 4, 5, 6]), None, "compressed");
        assert_eq!(archive.content(&[4, 1, 2, 4]), None);
        assert_eq!(archive.resource(&key(7, 0)), Some(vec![6, 7, 8, 9]));
        assert_eq!(archive.resource(&key(8, 0)), Some(vec![10, 11, 12]));
        assert_eq!(archive.resource(&key(8, 1)), None);
        assert!(archive.contains(&key(8, 0)));
        assert!(!archive.contains(&key(8, 1)));
        archive.trim();
        assert_eq!(archive.resource(&key(7, 0)), Some(vec![6, 7, 8, 9]));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "unpacking archive chunks takes minutes under Miri")]
    fn counts_and_sizes_past_the_file_fail_the_read() {
        let (dir, archive) = archive();
        let mut h = std::fs::read(dir.join("game.kfc")).unwrap();
        for field in [RESOURCE_KEYS, CHUNKS] {
            let mut broken = h.clone();
            put(&mut broken, field + 4, u32::MAX);
            if field == RESOURCE_KEYS {
                put(&mut broken, RESOURCE_VALUES + 4, u32::MAX);
            }
            assert!(Archive::from_bytes(broken, Vec::new(), []).is_none());
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
    #[cfg_attr(miri, ignore = "unpacking archive chunks takes minutes under Miri")]
    fn another_format_does_not_open() {
        let mut header = vec![0u8; 0x100];
        header[..4].copy_from_slice(b"KFC2");
        assert!(Archive::from_bytes(header, Vec::new(), []).is_none());
        assert!(Archive::from_bytes(Vec::new(), Vec::new(), []).is_none());
    }

    #[test]
    fn a_stored_length_gets_back_the_multiples_of_2_24_that_fit_before_the_next_resource() {
        assert_eq!(
            uncut(0xa9_748c, 0x2a9_7490),
            0x2a9_748c,
            "the temperature part"
        );
        assert_eq!(uncut(0xa9_748c, 0xa9_7490), 0xa9_748c, "padded to 16 bytes");
        assert_eq!(uncut(0xa9_748c, 0xa9_748c + CUT - 1), 0xa9_748c);
        assert_eq!(uncut(0xa9_748c, 0xa9_748c + CUT), 0xa9_748c + CUT);
        assert_eq!(uncut(16, 4), 16, "overlapping the next resource");
        assert_eq!(uncut(CUT, u32::MAX), CUT, "not cut");
        assert_eq!(uncut(0, CUT + 8), CUT, "exactly 2^24, stored as 0");
    }

    #[test]
    fn a_stored_length_stays_where_the_room_after_it_is_not_its_own() {
        assert_eq!(
            uncut(0, 0x2a9_7490),
            0,
            "a part sharing the temperature part's offset"
        );
        assert_eq!(uncut(0x100, 0x100 + CUT + 0x1000), 0x100, "a gap after it");
        assert_eq!(uncut(0x100, 0x100 + CUT + ALIGN), 0x100, "past the padding");
    }

    #[test]
    #[cfg_attr(miri, ignore = "reads 16 MiB, which takes minutes under Miri")]
    fn a_resource_whose_length_the_map_cuts_reads_whole() {
        let whole = CUT + 16;
        let mut h = vec![0u8; 0x100];
        put(&mut h, 0, MAGIC);
        locate(&mut h, RESOURCE_KEYS, 0x90, 2);
        locate(&mut h, RESOURCE_VALUES, 0xd0, 2);
        h[0x90..0xa0].copy_from_slice(&[7; 16]);
        put(&mut h, 0xa0, 0xabcd);
        h[0xb0..0xc0].copy_from_slice(&[8; 16]);
        put(&mut h, 0xc0, 0xabcd);
        // The first resource holds `whole` bytes; the map keeps 16 of them.
        put(&mut h, 0xd0, 0);
        put(&mut h, 0xd4, 16);
        put(&mut h, 0xd8, whole);
        put(&mut h, 0xdc, 4);
        locate(&mut h, CHUNKS, 0xe0, 1);
        for (at, value) in [(0xe4, whole + 4), (0xe8, whole + 4), (0xf0, whole + 4)] {
            put(&mut h, at, value);
        }
        let whole = whole as usize;
        let mut resources = vec![0u8; whole + 4];
        resources[whole - 1] = 1;
        resources[whole..].copy_from_slice(&[2, 3, 4, 5]);
        let archive = Archive::from_bytes(h, resources, []).unwrap();
        let first = archive.resource(&key(7, 0)).unwrap();
        assert_eq!((first.len(), first.last()), (whole, Some(&1)));
        assert_eq!(archive.resource(&key(8, 0)), Some(vec![2, 3, 4, 5]));
        assert_eq!(
            archive.uncut_len(&key(7, 0), 16),
            whole,
            "the game's cut length"
        );
        assert_eq!(archive.uncut_len(&key(7, 0), whole), whole);
        assert_eq!(archive.uncut_len(&key(7, 0), 17), 17, "another length");
        assert_eq!(archive.uncut_len(&key(8, 0), 4), 4);
        assert_eq!(
            archive.uncut_len(&key(8, 1), 4),
            4,
            "a key the archive lacks"
        );
    }

    /// The installed client's archive, when `ENSHWICK_CLIENT_DIR` names its
    /// folder and it holds the volume: the temperature volume, whose length
    /// the map of client 26430625 stores cut to 24 bits, reads as long as
    /// its own header says it is.
    #[test]
    #[cfg_attr(miri, ignore = "unpacking archive chunks takes minutes under Miri")]
    fn the_client_s_temperature_volume_reads_as_long_as_its_header_says() {
        use enshwick::typed::{Named, VoxelTemperatureResource};

        /// Where the volume header keeps its data's offset, relative to the
        /// field, and the data's length in bytes.
        const DATA: usize = 0x24;
        const DATA_LEN: usize = 0x28;
        let Ok(dir) = std::env::var("ENSHWICK_CLIENT_DIR") else {
            return;
        };
        let archive = Archive::open(Path::new(&dir), "enshrouded").unwrap();
        let name = crate::catalogue::type_name(VoxelTemperatureResource::ID);
        let key = ResourceKey {
            guid: Guid([
                0xdb, 0xea, 0x9f, 0x50, 0x60, 0x4c, 0x5f, 0x42, 0x9c, 0x7c, 0xde, 0xee, 0xfd, 0x9b,
                0x69, 0x20,
            ]),
            type_hash: crate::reflect::fnv1a32(name.as_bytes()),
            part: 3,
        };
        let Some(volume) = archive.resource(&key) else {
            return;
        };
        let data = DATA + word(&volume, DATA).unwrap() as usize;
        let data_len = word(&volume, DATA_LEN).unwrap() as usize;
        assert_eq!(volume.len(), data + data_len, "the header's own length");
        assert_eq!(
            archive.uncut_len(&key, volume.len() % CUT as usize),
            volume.len(),
            "from the map's cut length"
        );
    }

    /// The installed client's archive, when `ENSHWICK_CLIENT_DIR` names its
    /// folder (build 1269579): the Blacksmith's staff icon reads whole and
    /// decodes, and the item registry lists the Workbench.
    #[test]
    #[cfg_attr(miri, ignore = "unpacking archive chunks takes minutes under Miri")]
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
        let hash =
            crate::catalogue::type_hash(enshwick::content::types::ITEM_REGISTRY_RESOURCE.id());
        let registry = enshwick::content::items::ITEM_REGISTRY.key_by_hash(hash.unwrap());
        let registry = archive.resource(&registry).unwrap();
        let workbench = enshwick::content::workshops::stations::WORKBENCH.item;
        assert!(registry.windows(16).any(|w| w == workbench.0));
    }
}
