//! The map markers a scene places, read from its chunk and template
//! resources in the game's archive.
//!
//! A scene's chunk resource lists the templates its entities use and, for
//! each entity, the template's index and a packed transform. A template
//! resource lists its components as blob variants; a marker is a template
//! with a `MapMarker` among them. Field offsets and sizes come from the
//! game's reflection; the blob formats are the archive's own: an array is
//! `{i32 offset from the field, u32 count}` and a variant `{u32 type hash,
//! i32 offset of the data from its second word, u32 size}`.

/// Bytes of an archive blob variant.
const VARIANT_SIZE: usize = 12;
/// Bytes of a GUID, the size of a template reference.
const GUID_SIZE: usize = 16;

/// Where the reflected types keep what the parse reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneLayout {
    /// `SceneEntityChunkResource.templates`.
    pub templates: usize,
    /// `SceneEntityChunkResource.entities`.
    pub entities: usize,
    /// The size of a `SceneEntityChunkSpawnEntry`.
    pub entry_size: usize,
    /// `SceneEntityChunkSpawnEntry.templateIndex`.
    pub entry_template: usize,
    /// `SceneEntityChunkSpawnEntry.transform`.
    pub entry_transform: usize,
    /// `SceneEntityTransform.positionX0` to `positionZ1`, low and high
    /// halves of each axis in that order.
    pub position: [usize; 6],
    /// `TemplateResource.components`.
    pub components: usize,
    /// `MapMarker.markerId`.
    pub marker_id: usize,
    /// `MapMarker.markerType`.
    pub marker_type: usize,
    /// The size of a `MapMarker`.
    pub marker_size: usize,
}

/// An entity a chunk places: its template and where, in metres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    /// The template's GUID, in memory order.
    pub template: [u8; 16],
    /// Where it stands, in metres, y up.
    pub position: [f64; 3],
}

/// What a template's `MapMarker` holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkerData {
    /// The label's text tag GUID, `markerId`.
    pub label: [u8; 16],
    /// The type reference, `markerType`.
    pub marker_type: [u8; 16],
}

fn bytes<const N: usize>(data: &[u8], at: usize) -> Option<[u8; N]> {
    data.get(at..at.checked_add(N)?)?.try_into().ok()
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    bytes(data, at).map(u16::from_le_bytes)
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    bytes(data, at).map(u32::from_le_bytes)
}

fn i32_at(data: &[u8], at: usize) -> Option<i32> {
    bytes(data, at).map(i32::from_le_bytes)
}

/// The start and count of the blob array whose header sits at `field`;
/// `(0, 0)` for an empty one, `None` when it points outside `data`.
fn array(data: &[u8], field: usize, element: usize) -> Option<(usize, usize)> {
    let offset = i32_at(data, field)?;
    let count = usize::try_from(u32_at(data, field + 4)?).ok()?;
    if offset == 0 || count == 0 {
        return Some((0, 0));
    }
    let start = field.checked_add_signed(isize::try_from(offset).ok()?)?;
    let end = start.checked_add(count.checked_mul(element)?)?;
    (end <= data.len()).then_some((start, count))
}

impl SceneLayout {
    /// Whether the layout can be one the game has: an entry holds its
    /// template index and every position half, and a marker its two GUIDs.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::scene_markers::SceneLayout;
    ///
    /// let layout = SceneLayout {
    ///     templates: 0, entities: 8, entry_size: 20, entry_template: 0,
    ///     entry_transform: 2, position: [0, 2, 4, 6, 8, 10],
    ///     components: 16, marker_id: 0, marker_type: 16, marker_size: 40,
    /// };
    /// assert!(layout.is_sound());
    /// assert!(!SceneLayout { entry_size: 0, ..layout }.is_sound());
    /// ```
    #[must_use]
    pub fn is_sound(&self) -> bool {
        let fits = |at: usize, width: usize| {
            at.checked_add(width)
                .is_some_and(|end| end <= self.entry_size)
        };
        let furthest = self.position.iter().copied().max().unwrap_or(0);
        let in_marker = |at: usize| {
            at.checked_add(GUID_SIZE)
                .is_some_and(|end| end <= self.marker_size)
        };
        self.entry_size > 0
            && in_marker(self.marker_id)
            && in_marker(self.marker_type)
            && fits(self.entry_template, 2)
            && self
                .entry_transform
                .checked_add(furthest)
                .is_some_and(|at| fits(at, 2))
    }
}

/// Every entity the chunk resource `chunk` places; `None` when the layout
/// is not sound or the chunk does not read as one.
///
/// # Examples
///
/// ```
/// use enshwick_loader::scene_markers::{SceneLayout, placements};
///
/// let layout = SceneLayout {
///     templates: 0, entities: 8, entry_size: 20, entry_template: 0,
///     entry_transform: 2, position: [0, 2, 4, 6, 8, 10],
///     components: 16, marker_id: 0, marker_type: 16, marker_size: 40,
/// };
/// assert_eq!(placements(&layout, &[0; 16]), Some(Vec::new()));
/// assert_eq!(placements(&layout, &[0; 3]), None);
/// ```
#[must_use]
pub fn placements(layout: &SceneLayout, chunk: &[u8]) -> Option<Vec<Placement>> {
    if !layout.is_sound() {
        return None;
    }
    let (templates, template_count) = array(chunk, layout.templates, GUID_SIZE)?;
    let (entities, entity_count) = array(chunk, layout.entities, layout.entry_size)?;
    let mut placed = Vec::with_capacity(entity_count);
    for index in 0..entity_count {
        let entry = entities + index * layout.entry_size;
        let template = usize::from(u16_at(chunk, entry + layout.entry_template)?);
        if template >= template_count {
            return None;
        }
        let transform = entry + layout.entry_transform;
        let half = |at: usize| u16_at(chunk, transform + layout.position[at]).map(f64::from);
        let axis = |low: usize| Some(half(low + 1)? + half(low)? / 65_536.0);
        placed.push(Placement {
            template: bytes(chunk, templates + template * GUID_SIZE)?,
            position: [axis(0)?, axis(2)?, axis(4)?],
        });
    }
    Some(placed)
}

/// A template resource that does not read as one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the template's components or its marker do not read")]
pub struct ReadTemplateError;

/// The `MapMarker` of the template resource `template`, found by its type
/// hash `marker`; `Ok(None)` for a template without one.
///
/// # Errors
///
/// [`ReadTemplateError`] for a template whose components or marker do not read.
///
/// # Examples
///
/// ```
/// use enshwick_loader::scene_markers::{SceneLayout, marker};
///
/// let layout = SceneLayout {
///     templates: 0, entities: 8, entry_size: 20, entry_template: 0,
///     entry_transform: 2, position: [0, 2, 4, 6, 8, 10],
///     components: 16, marker_id: 0, marker_type: 16, marker_size: 40,
/// };
/// assert_eq!(marker(&layout, &[0; 24], 0x8af0_70c5), Ok(None));
/// assert!(marker(&layout, &[0; 3], 0x8af0_70c5).is_err());
/// ```
pub fn marker(
    layout: &SceneLayout,
    template: &[u8],
    marker: u32,
) -> Result<Option<MarkerData>, ReadTemplateError> {
    let (start, count) =
        array(template, layout.components, VARIANT_SIZE).ok_or(ReadTemplateError)?;
    let Some(variant) = (0..count)
        .map(|index| start + index * VARIANT_SIZE)
        .find(|&variant| u32_at(template, variant) == Some(marker))
    else {
        return Ok(None);
    };
    let read = || {
        let offset = isize::try_from(i32_at(template, variant + 4)?).ok()?;
        let data = (variant + 4).checked_add_signed(offset)?;
        let size = usize::try_from(u32_at(template, variant + 8)?).ok()?;
        let need = layout
            .marker_id
            .max(layout.marker_type)
            .checked_add(GUID_SIZE)?;
        if size < need || data.checked_add(size)? > template.len() {
            return None;
        }
        Some(MarkerData {
            label: bytes(template, data + layout.marker_id)?,
            marker_type: bytes(template, data + layout.marker_type)?,
        })
    };
    read().map(Some).ok_or(ReadTemplateError)
}

/// More chunks than any scene has: a count past it is a misread.
pub const MAX_CHUNKS: u64 = 1 << 20;

/// A scene's global chunk: its scene GUID and chunk part, the last of the
/// scene's chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SceneKey {
    /// The scene's GUID, in memory order.
    pub scene: [u8; 16],
    /// The global chunk's part.
    pub part: u32,
}

impl SceneKey {
    /// The global chunk of `scene` with `count` chunks; `None` for a zero
    /// GUID or a count no scene has.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::scene_markers::SceneKey;
    ///
    /// assert_eq!(SceneKey::new([7; 16], 1601).map(|key| key.part), Some(1600));
    /// assert_eq!(SceneKey::new([0; 16], 1601), None);
    /// ```
    #[must_use]
    pub fn new(scene: [u8; 16], count: u64) -> Option<Self> {
        if scene == [0; 16] || count == 0 || count > MAX_CHUNKS {
            return None;
        }
        let part = u32::try_from(count - 1).ok()?;
        Some(Self { scene, part })
    }
}

/// The step at which a scene's markers did not read.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReadSceneError {
    /// The reflected types the parse needs are not all there.
    #[error("the reflected scene types are missing")]
    Layout,
    /// The read panicked.
    #[error("the read panicked")]
    Panicked,
    /// The game's archive is not open.
    #[error("the game's archive is not open")]
    Archive,
    /// The archive lacks the scene's global chunk.
    #[error("the archive lacks the scene's global chunk")]
    Chunk,
    /// The global chunk does not read as one.
    #[error("the scene's global chunk does not read")]
    ChunkUnreadable,
    /// The chunk places entities, but none of their templates read.
    #[error("none of the global chunk's templates read")]
    Templates,
    /// No thread could be started to read the scene.
    #[error("no thread could be started to read the scene")]
    Thread,
}

/// Templates a scene names that the marker read skipped.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Skipped {
    /// Templates the archive lacks.
    pub missing: usize,
    /// Templates that do not read.
    pub unreadable: usize,
}

/// The markers among `placed`, each with its template's marker, reading
/// each distinct template once with `template`.
///
/// # Errors
///
/// [`ReadSceneError::Templates`] when entities are placed and none of their
/// templates read.
///
/// # Examples
///
/// ```
/// use enshwick_loader::scene_markers::{SceneLayout, collect};
///
/// let layout = SceneLayout {
///     templates: 0, entities: 8, entry_size: 20, entry_template: 0,
///     entry_transform: 2, position: [0, 2, 4, 6, 8, 10],
///     components: 16, marker_id: 0, marker_type: 16, marker_size: 40,
/// };
/// let (markers, skipped) = collect(&layout, &[], 0x8af0_70c5, |_| None)?;
/// assert!(markers.is_empty() && skipped.missing == 0);
/// # Ok::<(), enshwick_loader::scene_markers::ReadSceneError>(())
/// ```
pub fn collect(
    layout: &SceneLayout,
    placed: &[Placement],
    marker_hash: u32,
    mut template: impl FnMut([u8; 16]) -> Option<Vec<u8>>,
) -> Result<(Vec<(Placement, MarkerData)>, Skipped), ReadSceneError> {
    let mut seen: rustc_hash::FxHashMap<[u8; 16], Option<MarkerData>> =
        rustc_hash::FxHashMap::default();
    let mut skipped = Skipped::default();
    let mut read = 0usize;
    let mut markers = Vec::new();
    for placement in placed {
        let found = *seen.entry(placement.template).or_insert_with(|| {
            let Some(bytes) = template(placement.template) else {
                skipped.missing += 1;
                return None;
            };
            if let Ok(found) = marker(layout, &bytes, marker_hash) {
                read += 1;
                found
            } else {
                skipped.unreadable += 1;
                None
            }
        });
        if let Some(data) = found {
            markers.push((*placement, data));
        }
    }
    if !placed.is_empty() && read == 0 {
        return Err(ReadSceneError::Templates);
    }
    Ok((markers, skipped))
}

/// A scene's markers, or why they did not read.
pub type SceneRead<T> = Result<std::sync::Arc<[T]>, ReadSceneError>;

/// The markers of the last scene asked for, or why they did not read,
/// kept until the scene changes; a read runs apart from the caller.
#[derive(Debug)]
pub struct SceneMarkers<T> {
    held: Option<(SceneKey, Option<SceneRead<T>>)>,
}

/// What a [`SceneMarkers::lookup`] found.
#[derive(Debug, PartialEq, Eq)]
pub enum Lookup<T> {
    /// The scene's markers, or why they did not read.
    Ready(SceneRead<T>),
    /// A read of the scene is under way.
    Pending,
    /// The scene is new: the caller starts its read and calls
    /// [`SceneMarkers::finish`] with it.
    Start,
}

impl<T> SceneMarkers<T> {
    /// Nothing read yet.
    #[must_use]
    pub const fn new() -> Self {
        Self { held: None }
    }

    /// The markers of `key`; for a scene not held yet, it is held as being
    /// read from now on, and only this call answers [`Lookup::Start`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick_loader::scene_markers::{Lookup, SceneKey, SceneMarkers};
    ///
    /// let mut cache = SceneMarkers::new();
    /// let key = SceneKey::new([1; 16], 2).unwrap();
    /// assert_eq!(cache.lookup(key), Lookup::Start);
    /// assert_eq!(cache.lookup(key), Lookup::Pending);
    /// assert!(cache.finish(key, Ok(vec![5u8])));
    /// assert!(matches!(cache.lookup(key), Lookup::Ready(Ok(markers)) if markers.len() == 1));
    /// ```
    pub fn lookup(&mut self, key: SceneKey) -> Lookup<T> {
        match &self.held {
            Some((held, Some(found))) if *held == key => Lookup::Ready(found.clone()),
            Some((held, None)) if *held == key => Lookup::Pending,
            _ => {
                self.held = Some((key, None));
                Lookup::Start
            }
        }
    }

    /// Keeps the read of `key`, unless the scene changed meanwhile; whether
    /// it was kept.
    pub fn finish(&mut self, key: SceneKey, read: Result<Vec<T>, ReadSceneError>) -> bool {
        match &mut self.held {
            Some((held, slot @ None)) if *held == key => {
                *slot = Some(read.map(std::sync::Arc::from));
                true
            }
            _ => false,
        }
    }
}

impl<T> Default for SceneMarkers<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const LAYOUT: SceneLayout = SceneLayout {
        templates: 0,
        entities: 0x18,
        entry_size: 20,
        entry_template: 0,
        entry_transform: 2,
        position: [0, 2, 4, 6, 8, 10],
        components: 0x10,
        marker_id: 0,
        marker_type: 0x10,
        marker_size: 40,
    };
    const MARKER: u32 = 0x8af0_70c5;

    /// A chunk of `templates` and `entities` (template index, x, y, z in
    /// whole and 1/65536 metres), laid out as the archive does.
    fn chunk(templates: &[[u8; 16]], entities: &[(u16, [(u16, u16); 3])]) -> Vec<u8> {
        let mut data = vec![0u8; 0x20];
        let put_array = |data: &mut Vec<u8>, field: usize, count: usize| {
            let start = data.len();
            let offset = i32::try_from(start - field).unwrap();
            data[field..field + 4].copy_from_slice(&offset.to_le_bytes());
            data[field + 4..field + 8]
                .copy_from_slice(&u32::try_from(count).unwrap().to_le_bytes());
        };
        put_array(&mut data, 0, templates.len());
        for guid in templates {
            data.extend_from_slice(guid);
        }
        put_array(&mut data, 0x18, entities.len());
        for &(template, axes) in entities {
            data.extend_from_slice(&template.to_le_bytes());
            for (whole, fraction) in axes {
                data.extend_from_slice(&fraction.to_le_bytes());
                data.extend_from_slice(&whole.to_le_bytes());
            }
            data.extend_from_slice(&[0; 6]);
        }
        data
    }

    /// A template resource with components `(hash, data)`.
    fn template(components: &[(u32, Vec<u8>)]) -> Vec<u8> {
        let mut data = vec![0u8; 24];
        let start = data.len();
        data[0x10..0x14].copy_from_slice(&i32::try_from(start - 0x10).unwrap().to_le_bytes());
        data[0x14..0x18].copy_from_slice(&u32::try_from(components.len()).unwrap().to_le_bytes());
        let table = data.len();
        data.resize(table + components.len() * VARIANT_SIZE, 0);
        for (index, (hash, body)) in components.iter().enumerate() {
            let at = table + index * VARIANT_SIZE;
            let offset = i32::try_from(data.len() - (at + 4)).unwrap();
            data[at..at + 4].copy_from_slice(&hash.to_le_bytes());
            data[at + 4..at + 8].copy_from_slice(&offset.to_le_bytes());
            data[at + 8..at + 12]
                .copy_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
            data.extend_from_slice(body);
        }
        data
    }

    #[test]
    fn a_chunk_places_each_entity_with_its_template_and_position() {
        let data = chunk(
            &[[1; 16], [2; 16]],
            &[
                (1, [(4195, 0x9999), (834, 0), (1855, 0x8000)]),
                (0, [(10, 0), (0, 0), (10_239, 0xffff)]),
            ],
        );
        let placed = placements(&LAYOUT, &data).unwrap();
        assert_eq!(placed.len(), 2);
        assert_eq!(placed[0].template, [2; 16]);
        assert!((placed[0].position[0] - 4195.6).abs() < 0.001);
        assert_eq!(placed[0].position[1..], [834.0, 1855.5]);
        assert_eq!(placed[1].template, [1; 16]);
    }

    #[test]
    fn a_chunk_naming_a_template_it_lacks_or_cut_short_does_not_read() {
        let data = chunk(&[[1; 16]], &[(1, [(0, 0); 3])]);
        assert_eq!(placements(&LAYOUT, &data), None);
        let data = chunk(&[[1; 16]], &[(0, [(0, 0); 3])]);
        assert_eq!(placements(&LAYOUT, &data[..data.len() - 1]), None);
    }

    #[test]
    fn a_template_gives_its_marker_and_one_without_gives_none() {
        let mut body = vec![3u8; 16];
        body.extend([4u8; 16]);
        body.extend([0u8; 8]);
        let with = template(&[(0x1234, vec![9; 4]), (MARKER, body)]);
        assert_eq!(
            marker(&LAYOUT, &with, MARKER),
            Ok(Some(MarkerData {
                label: [3; 16],
                marker_type: [4; 16]
            }))
        );
        let without = template(&[(0x1234, vec![9; 4])]);
        assert_eq!(marker(&LAYOUT, &without, MARKER), Ok(None));
        let short = template(&[(MARKER, vec![1; 20])]);
        assert_eq!(marker(&LAYOUT, &short, MARKER), Err(ReadTemplateError));
    }

    proptest! {
        #[test]
        fn any_bytes_parse_without_a_panic(data in prop::collection::vec(any::<u8>(), 0..512)) {
            let _placed = placements(&LAYOUT, &data);
            let _marker = marker(&LAYOUT, &data, MARKER);
        }
    }

    #[test]
    fn a_layout_with_no_room_for_its_fields_is_refused_before_any_count_is_trusted() {
        let mut data = vec![0u8; 0x20];
        data[0x18..0x1c].copy_from_slice(&8i32.to_le_bytes());
        data[0x1c..0x20].copy_from_slice(&u32::MAX.to_le_bytes());
        let empty = SceneLayout {
            entry_size: 0,
            ..LAYOUT
        };
        assert_eq!(placements(&empty, &data), None);
        let short = SceneLayout {
            entry_size: 11,
            ..LAYOUT
        };
        assert_eq!(placements(&short, &data), None);
    }

    #[test]
    fn a_layout_whose_marker_guids_leave_the_marker_is_refused() {
        assert!(LAYOUT.is_sound());
        assert!(
            !SceneLayout {
                marker_size: 31,
                ..LAYOUT
            }
            .is_sound()
        );
        assert!(
            !SceneLayout {
                marker_id: usize::MAX,
                ..LAYOUT
            }
            .is_sound()
        );
    }

    #[test]
    fn a_scene_key_takes_the_last_chunk_of_a_count_a_scene_can_have() {
        assert_eq!(
            SceneKey::new([1; 16], 1),
            Some(SceneKey {
                scene: [1; 16],
                part: 0
            })
        );
        assert_eq!(
            SceneKey::new([1; 16], MAX_CHUNKS).map(|key| u64::from(key.part)),
            Some(MAX_CHUNKS - 1)
        );
        assert_eq!(SceneKey::new([1; 16], MAX_CHUNKS + 1), None);
        assert_eq!(SceneKey::new([1; 16], 0), None);
        assert_eq!(SceneKey::new([0; 16], 5), None);
    }

    #[test]
    fn a_failure_is_kept_for_its_scene_and_a_new_scene_reads_again() {
        let mut cache: SceneMarkers<u8> = SceneMarkers::new();
        let first = SceneKey::new([1; 16], 2).unwrap();
        let second = SceneKey::new([2; 16], 2).unwrap();
        assert_eq!(cache.lookup(first), Lookup::Start);
        assert!(cache.finish(first, Err(ReadSceneError::Chunk)));
        assert_eq!(
            cache.lookup(first),
            Lookup::Ready(Err(ReadSceneError::Chunk))
        );
        assert_eq!(cache.lookup(second), Lookup::Start);
        assert!(cache.finish(second, Ok(vec![1, 2])));
        assert!(matches!(cache.lookup(second), Lookup::Ready(Ok(m)) if m.len() == 2));
    }

    #[test]
    fn a_read_of_a_scene_left_meanwhile_is_dropped() {
        let mut cache: SceneMarkers<u8> = SceneMarkers::new();
        let first = SceneKey::new([1; 16], 2).unwrap();
        let second = SceneKey::new([2; 16], 2).unwrap();
        assert_eq!(cache.lookup(first), Lookup::Start);
        assert_eq!(cache.lookup(second), Lookup::Start);
        assert!(!cache.finish(first, Ok(vec![1])));
        assert_eq!(cache.lookup(second), Lookup::Pending);
    }

    #[test]
    fn templates_are_read_once_and_skips_are_counted() {
        let mut body = vec![3u8; 16];
        body.extend([4u8; 16]);
        let with = template(&[(MARKER, body)]);
        let without = template(&[(0x1234, vec![9; 4])]);
        let place = |t: u8| Placement {
            template: [t; 16],
            position: [0.0; 3],
        };
        let placed = [place(1), place(2), place(1), place(3), place(4)];
        let mut reads = 0;
        let (markers, skipped) = collect(&LAYOUT, &placed, MARKER, |guid| {
            reads += 1;
            match guid[0] {
                1 => Some(with.clone()),
                2 => Some(without.clone()),
                3 => Some(vec![0; 3]),
                _ => None,
            }
        })
        .unwrap();
        assert_eq!(markers.len(), 2);
        assert_eq!(
            skipped,
            Skipped {
                missing: 1,
                unreadable: 1
            }
        );
        assert_eq!(reads, 4);
    }

    #[test]
    fn a_chunk_whose_templates_never_read_fails() {
        let placed = [Placement {
            template: [1; 16],
            position: [0.0; 3],
        }];
        assert_eq!(
            collect(&LAYOUT, &placed, MARKER, |_| None),
            Err(ReadSceneError::Templates)
        );
        assert!(collect(&LAYOUT, &[], MARKER, |_| None).is_ok());
    }
}
