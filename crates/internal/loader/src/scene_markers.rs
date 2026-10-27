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

/// Every entity the chunk resource `chunk` places; `None` when it does not
/// read as one.
///
/// # Examples
///
/// ```
/// use enshwick_loader::scene_markers::{SceneLayout, placements};
///
/// let layout = SceneLayout {
///     templates: 0, entities: 8, entry_size: 20, entry_template: 0,
///     entry_transform: 2, position: [0, 2, 4, 6, 8, 10],
///     components: 16, marker_id: 0, marker_type: 16,
/// };
/// assert_eq!(placements(&layout, &[0; 16]), Some(Vec::new()));
/// assert_eq!(placements(&layout, &[0; 3]), None);
/// ```
#[must_use]
pub fn placements(layout: &SceneLayout, chunk: &[u8]) -> Option<Vec<Placement>> {
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

/// The `MapMarker` of the template resource `template`, found by its type
/// hash `marker`; `None` for a template without one or that does not read.
///
/// # Examples
///
/// ```
/// use enshwick_loader::scene_markers::{SceneLayout, marker};
///
/// let layout = SceneLayout {
///     templates: 0, entities: 8, entry_size: 20, entry_template: 0,
///     entry_transform: 2, position: [0, 2, 4, 6, 8, 10],
///     components: 16, marker_id: 0, marker_type: 16,
/// };
/// assert_eq!(marker(&layout, &[0; 24], 0x8af0_70c5), None);
/// ```
#[must_use]
pub fn marker(layout: &SceneLayout, template: &[u8], marker: u32) -> Option<MarkerData> {
    let (start, count) = array(template, layout.components, VARIANT_SIZE)?;
    (0..count)
        .map(|index| start + index * VARIANT_SIZE)
        .find(|&variant| u32_at(template, variant) == Some(marker))
        .and_then(|variant| {
            let offset = isize::try_from(i32_at(template, variant + 4)?).ok()?;
            let data = (variant + 4).checked_add_signed(offset)?;
            let size = usize::try_from(u32_at(template, variant + 8)?).ok()?;
            let need = layout.marker_id.max(layout.marker_type) + GUID_SIZE;
            if size < need || data.checked_add(size)? > template.len() {
                return None;
            }
            Some(MarkerData {
                label: bytes(template, data + layout.marker_id)?,
                marker_type: bytes(template, data + layout.marker_type)?,
            })
        })
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
            Some(MarkerData {
                label: [3; 16],
                marker_type: [4; 16]
            })
        );
        let without = template(&[(0x1234, vec![9; 4])]);
        assert_eq!(marker(&LAYOUT, &without, MARKER), None);
        let short = template(&[(MARKER, vec![1; 20])]);
        assert_eq!(marker(&LAYOUT, &short, MARKER), None);
    }

    proptest! {
        #[test]
        fn any_bytes_parse_without_a_panic(data in prop::collection::vec(any::<u8>(), 0..512)) {
            let _placed = placements(&LAYOUT, &data);
            let _marker = marker(&LAYOUT, &data, MARKER);
        }
    }
}
