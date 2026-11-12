//! What the server administration tools answer, from what the game thread
//! read.

use std::fmt::Write as _;

use enshwick_core::abi::Side;
use enshwick_core::content::Guid;

/// Placed pieces [`Placed`] keeps, newest last.
pub const REMEMBERED: usize = 32;

/// A player online, for `enshwick world`.
#[derive(Clone, Debug, PartialEq)]
pub struct Online {
    /// The name the client sent.
    pub name: String,
    /// The server's slot.
    pub slot: u32,
    /// The character's entity.
    pub entity: u32,
    /// The account.
    pub key: u64,
    /// Whether the player is an administrator.
    pub admin: bool,
    /// Where the character stands, in metres.
    pub position: Option<[f64; 3]>,
}

/// What `enshwick world` answers.
#[must_use]
pub fn world(players: &[Online]) -> String {
    let mut out = match players.len() {
        0 => "no players online".to_owned(),
        1 => "1 player online".to_owned(),
        n => format!("{n} players online"),
    };
    for p in players {
        let admin = if p.admin { ", admin" } else { "" };
        let _w = write!(
            out,
            "\n{} (account {:016x}, slot {}, entity {}{admin}) at {}",
            p.name,
            p.key,
            p.slot,
            p.entity,
            place(p.position)
        );
    }
    out
}

/// What the server knows of an entity, for `enshwick entity`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Seen {
    /// The entity.
    pub entity: u32,
    /// Its template GUID, in file order.
    pub template: Option<[u8; 16]>,
    /// The item it was placed from.
    pub item: Option<u32>,
    /// Where it is, in metres.
    pub position: Option<[f64; 3]>,
    /// The player whose character it is.
    pub player: Option<String>,
}

/// What `enshwick entity` answers, with each item in `item_words`.
#[must_use]
pub fn entity(seen: &Seen, item_words: impl Fn(u32) -> String) -> String {
    if seen.template.is_none() && seen.item.is_none() && seen.position.is_none() {
        return format!("entity {}: the server knows nothing of it", seen.entity);
    }
    let mut out = format!(
        "entity {}: template {}, item {}, at {}",
        seen.entity,
        seen.template
            .map_or_else(|| "-".to_owned(), |t| Guid(t).to_string()),
        seen.item.map_or_else(|| "-".to_owned(), item_words),
        place(seen.position)
    );
    if let Some(player) = &seen.player {
        let _w = write!(out, ", the character of {player}");
    }
    out
}

/// A piece a player placed, for `enshwick placed`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    /// The placed entity.
    pub entity: u32,
    /// The item placed.
    pub item: u32,
    /// Its template GUID, in file order.
    pub template: [u8; 16],
    /// Where, in metres.
    pub position: [f32; 3],
}

/// What `enshwick placed` answers for `placed`, newest first, with each
/// item in `item_words`.
#[must_use]
pub fn placed(placed: &[Placed], item_words: impl Fn(u32) -> String) -> String {
    if placed.is_empty() {
        return "nothing placed since the server started".to_owned();
    }
    placed
        .iter()
        .map(|p| {
            format!(
                "entity {}: item {}, template {}, at {}",
                p.entity,
                item_words(p.item),
                Guid(p.template),
                place(Some(p.position.map(f64::from)))
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// How many of the newest placed pieces `enshwick placed <n>` asks for:
/// all kept without a number.
///
/// # Errors
///
/// [`ParseCountError`] with the word that is no number.
pub fn placed_count(args: &str) -> Result<usize, ParseCountError> {
    let args = args.trim();
    if args.is_empty() {
        return Ok(REMEMBERED);
    }
    args.parse::<usize>()
        .map(|n| n.clamp(1, REMEMBERED))
        .map_err(|source| ParseCountError {
            word: args.to_owned(),
            source,
        })
}

/// Why `enshwick placed` names no count of pieces.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("{word:?} is no count")]
pub struct ParseCountError {
    /// The word that is no number.
    pub word: String,
    #[source]
    source: core::num::ParseIntError,
}

/// A loaded mod, for `enshwick mods`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded<'a> {
    /// The mod id.
    pub id: &'a str,
    /// Its version.
    pub version: &'a str,
    /// Where it runs: players need a mod of side both to join.
    pub side: Side,
    /// Whether it runs, or was turned off.
    pub running: bool,
}

/// What `enshwick mods` answers.
#[must_use]
pub fn mods(loaded: &[Loaded<'_>]) -> String {
    if loaded.is_empty() {
        return "no mods loaded".to_owned();
    }
    loaded
        .iter()
        .map(|m| {
            let side = if m.side == Side::SERVER {
                "server"
            } else {
                "both"
            };
            let state = if m.running { "running" } else { "off" };
            format!("{} {} ({side}): {state}", m.id, m.version)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// `x, y, z` in metres to one decimal, or `-`.
#[must_use]
pub fn place(position: Option<[f64; 3]>) -> String {
    position.map_or_else(
        || "-".to_owned(),
        |[x, y, z]| format!("{x:.1}, {y:.1}, {z:.1}"),
    )
}

/// Metres from a position in 32.32 fixed-point metres.
#[must_use]
#[expect(
    clippy::cast_precision_loss,
    reason = "a position in metres needs far fewer digits than f64 keeps"
)]
pub fn metres(fixed: [i64; 3]) -> [f64; 3] {
    fixed.map(|v| v as f64 / 4_294_967_296.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_names_each_player_with_account_and_place() {
        assert_eq!(world(&[]), "no players online");
        let ann = Online {
            name: "Ann".into(),
            slot: 0,
            entity: 9,
            key: 0x2a,
            admin: true,
            position: Some([1.0, 2.5, -3.0]),
        };
        assert_eq!(
            world(&[ann]),
            "1 player online\nAnn (account 000000000000002a, slot 0, entity 9, admin) at 1.0, 2.5, -3.0"
        );
    }

    #[test]
    fn replies_name_what_the_server_knows() {
        assert_eq!(
            entity(
                &Seen {
                    entity: 7,
                    ..Seen::default()
                },
                |i| i.to_string()
            ),
            "entity 7: the server knows nothing of it"
        );
        let seen = Seen {
            entity: 12,
            template: Some(Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d").0),
            item: Some(77),
            position: Some([1.0, 2.0, 3.0]),
            player: Some("Ann".into()),
        };
        assert_eq!(
            entity(&seen, |i| format!("enshrouded:Tool_Torch01 ({i})")),
            "entity 12: template 6943053a-32c1-4718-ae22-e61427ddf54d, item enshrouded:Tool_Torch01 (77), at 1.0, 2.0, 3.0, the character of Ann"
        );
        let piece = Placed {
            entity: 5,
            item: 9,
            template: [0; 16],
            position: [0.5, 1.0, 2.0],
        };
        assert_eq!(
            placed(&[piece], |i| i.to_string()),
            "entity 5: item 9, template 00000000-0000-0000-0000-000000000000, at 0.5, 1.0, 2.0"
        );
        assert_eq!(
            placed(&[], |i| i.to_string()),
            "nothing placed since the server started"
        );
    }

    #[test]
    fn a_placed_count_stays_within_what_is_kept() {
        assert_eq!(placed_count(""), Ok(REMEMBERED));
        assert_eq!(placed_count(" 5 "), Ok(5));
        assert_eq!(placed_count("0"), Ok(1));
        assert_eq!(placed_count("999"), Ok(REMEMBERED));
        assert_eq!(placed_count("many").unwrap_err().word, "many");
    }

    #[test]
    fn mods_name_side_and_state() {
        assert_eq!(mods(&[]), "no mods loaded");
        let loaded = [
            Loaded {
                id: "you.trading-post",
                version: "0.3.0",
                side: Side::SERVER,
                running: true,
            },
            Loaded {
                id: "you.zones",
                version: "0.1.0",
                side: Side::BOTH,
                running: false,
            },
        ];
        assert_eq!(
            mods(&loaded),
            "you.trading-post 0.3.0 (server): running\nyou.zones 0.1.0 (both): off"
        );
    }

    #[test]
    fn a_fixed_point_position_reads_in_metres() {
        assert_eq!(metres([1 << 32, -(3 << 31), 0]), [1.0, -1.5, 0.0]);
    }
}
