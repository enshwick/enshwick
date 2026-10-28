//! The map as plain data, for a process outside the game.

use serde::{Deserialize, Serialize};

/// The format of [`MapSnapshot::to_bytes`]; it changes whenever a field of a
/// snapshot changes, and a reader refuses any other.
pub const MAP_SNAPSHOT_FORMAT: u16 = 1;

/// What the game server shows on a map at one moment, as plain data for a
/// process outside the game such as a web map beside a dedicated server.
///
/// It holds the players, the Flame Altars and the markers, in Enshwick's own
/// format, not the game's.
///
/// It carries no texts, since a dedicated server has none: a marker's label
/// is the id of its text tag, which a reader with the game's texts looks up.
///
/// # Examples
///
/// ```
/// use enshwick::map::{MapPlayer, MapSnapshot};
///
/// let mut snapshot = MapSnapshot::default();
/// snapshot.players.push(MapPlayer {
///     key: enshwick::PlayerKey::new(7),
///     name: "Bob".to_owned(),
///     position: Some([10.0, 40.0, -3.5]),
///     waypoint: None,
/// });
/// let bytes = snapshot.to_bytes()?;
/// assert_eq!(MapSnapshot::from_bytes(&bytes)?, snapshot);
/// # Ok::<(), enshwick::map::SnapshotError>(())
/// ```
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct MapSnapshot {
    /// The players in the game, by player slot.
    pub players: Vec<MapPlayer>,
    /// The Flame Altars, by entity.
    pub altars: Vec<MapAltar>,
    /// The world's map markers, by entity.
    pub markers: Vec<MapPoint>,
}

/// A player in a [`MapSnapshot`].
///
/// # Examples
///
/// See [`MapSnapshot`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapPlayer {
    /// The same for the same account in every session, as
    /// [`Player::key`](crate::world::Player::key); a public map may leave
    /// it out.
    pub key: crate::PlayerKey,
    /// The name the client sent; untrusted, may repeat between players.
    pub name: String,
    /// Where the character is, in metres; `None` while it has no place,
    /// such as while it loads.
    pub position: Option<[f64; 3]>,
    /// Where the player's waypoint is, in metres, while it is on.
    pub waypoint: Option<[f64; 3]>,
}

/// A Flame Altar in a [`MapSnapshot`].
///
/// # Examples
///
/// ```
/// use enshwick::map::MapAltar;
///
/// let altar = MapAltar {
///     position: [0.0, 30.0, 0.0],
///     level: 2,
///     zone: [[-40, 50, -40], [40, 90, 40]],
///     base: Some(0),
/// };
/// assert!(altar.zone[0][0] < altar.zone[1][0]);
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapAltar {
    /// Where it stands, in metres.
    pub position: [f64; 3],
    /// Its Flame level; 0 while unknown.
    pub level: u8,
    /// Its building zone, `[min, max]` in half-open 0.5 m cells, as
    /// [`CellBox`](crate::altars::CellBox) holds it.
    pub zone: [[i64; 3]; 2],
    /// The base it belongs to, numbered from 0 within the snapshot; altars
    /// whose zones touch share one. `None` for an altar with an empty zone,
    /// which forms no base.
    pub base: Option<u32>,
}

/// A map marker in a [`MapSnapshot`].
///
/// # Examples
///
/// ```
/// use enshwick::map::MapPoint;
///
/// let ruin = MapPoint { position: [512.0, 60.0, 2048.0], marker_type: Some(0x1234_5678), label: None };
/// assert!(ruin.label.is_none());
/// ```
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapPoint {
    /// Where it is, in metres.
    pub position: [f64; 3],
    /// Its [`MapMarkerTypeId`](super::MapMarkerTypeId) key.
    pub marker_type: Option<u32>,
    /// Its label's text id, [`Guid::fnv1a`](crate::content::Guid::fnv1a) of the
    /// tag.
    pub label: Option<u32>,
}

/// Why a [`MapSnapshot`] does not encode or read.
///
/// # Examples
///
/// ```
/// use enshwick::map::{MapSnapshot, SnapshotError};
///
/// assert_eq!(MapSnapshot::from_bytes(&[9, 0]), Err(SnapshotError::Format(9)));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SnapshotError {
    /// Written in another format than [`MAP_SNAPSHOT_FORMAT`].
    #[error("snapshot format {0}, this reader reads {MAP_SNAPSHOT_FORMAT}")]
    Format(u16),
    /// Cut short or not a snapshot.
    #[error("malformed snapshot")]
    Malformed,
    /// A value the encoding cannot hold.
    #[error("the snapshot does not encode")]
    Encode,
}

impl MapSnapshot {
    /// The snapshot as bytes: [`MAP_SNAPSHOT_FORMAT`] in two little-endian
    /// bytes, then the fields in postcard's encoding.
    ///
    /// # Errors
    ///
    /// [`SnapshotError::Encode`] when the encoder refuses a value.
    ///
    /// # Examples
    ///
    /// See [`MapSnapshot`].
    pub fn to_bytes(&self) -> Result<Vec<u8>, SnapshotError> {
        let bytes = MAP_SNAPSHOT_FORMAT.to_le_bytes().to_vec();
        postcard::to_extend(self, bytes).map_err(|_| SnapshotError::Encode)
    }

    /// Reads a snapshot [`MapSnapshot::to_bytes`] wrote.
    ///
    /// # Errors
    ///
    /// [`SnapshotError::Format`] for another format,
    /// [`SnapshotError::Malformed`] for bytes that are cut short, carry
    /// more than one snapshot or are not one.
    ///
    /// # Examples
    ///
    /// See [`MapSnapshot`].
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SnapshotError> {
        let (format, body) = bytes
            .split_first_chunk::<2>()
            .ok_or(SnapshotError::Malformed)?;
        let format = u16::from_le_bytes(*format);
        if format != MAP_SNAPSHOT_FORMAT {
            return Err(SnapshotError::Format(format));
        }
        match postcard::take_from_bytes(body) {
            Ok((snapshot, [])) => Ok(snapshot),
            _ => Err(SnapshotError::Malformed),
        }
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn snapshot() -> impl Strategy<Value = MapSnapshot> {
        let position = prop::array::uniform3(-1.0e5..1.0e5f64);
        let player = (
            any::<u64>(),
            ".{0,64}",
            prop::option::of(position.clone()),
            prop::option::of(position.clone()),
        )
            .prop_map(|(key, name, position, waypoint)| MapPlayer {
                key: crate::PlayerKey::new(key),
                name,
                position,
                waypoint,
            });
        let altar = (
            position.clone(),
            any::<u8>(),
            any::<[[i64; 3]; 2]>(),
            any::<Option<u32>>(),
        )
            .prop_map(|(position, level, zone, base)| MapAltar {
                position,
                level,
                zone,
                base,
            });
        let point = (position, any::<Option<u32>>(), any::<Option<u32>>()).prop_map(
            |(position, marker_type, label)| MapPoint {
                position,
                marker_type,
                label,
            },
        );
        (
            prop::collection::vec(player, 0..8),
            prop::collection::vec(altar, 0..8),
            prop::collection::vec(point, 0..32),
        )
            .prop_map(|(players, altars, markers)| MapSnapshot {
                players,
                altars,
                markers,
            })
    }

    proptest! {
        #[test]
        fn every_snapshot_reads_back_as_written(snapshot in snapshot()) {
            let bytes = snapshot.to_bytes().unwrap();
            prop_assert_eq!(MapSnapshot::from_bytes(&bytes).unwrap(), snapshot);
        }

        #[test]
        fn a_cut_snapshot_is_malformed(snapshot in snapshot(), cut in 1usize..64) {
            let bytes = snapshot.to_bytes().unwrap();
            let kept = bytes.len().saturating_sub(cut).max(2);
            prop_assume!(kept < bytes.len());
            prop_assert_eq!(MapSnapshot::from_bytes(&bytes[..kept]), Err(SnapshotError::Malformed));
        }
    }

    #[test]
    fn the_format_is_pinned_byte_for_byte() {
        let snapshot = MapSnapshot {
            players: vec![MapPlayer {
                key: crate::PlayerKey::new(1),
                name: "A".to_owned(),
                position: Some([0.5, 0.0, -1.0]),
                waypoint: None,
            }],
            altars: vec![MapAltar {
                position: [0.0; 3],
                level: 3,
                zone: [[-1, 0, -1], [1, 2, 1]],
                base: Some(0),
            }],
            markers: vec![MapPoint {
                position: [8.0, 0.0, 8.0],
                marker_type: Some(5),
                label: None,
            }],
        };
        let bytes = snapshot.to_bytes().unwrap();
        let hex = bytes.iter().fold(String::new(), |mut hex, byte| {
            use core::fmt::Write as _;
            let _infallible = write!(hex, "{byte:02x}");
            hex
        });
        insta::assert_snapshot!(hex);
    }

    #[test]
    fn trailing_bytes_are_malformed() {
        let mut bytes = MapSnapshot::default().to_bytes().unwrap();
        bytes.push(0);
        assert_eq!(
            MapSnapshot::from_bytes(&bytes),
            Err(SnapshotError::Malformed)
        );
        assert_eq!(MapSnapshot::from_bytes(&[1]), Err(SnapshotError::Malformed));
    }
}
