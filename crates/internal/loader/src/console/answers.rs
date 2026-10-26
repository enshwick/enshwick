//! What the server administration tools answer, from what the game thread
//! read.

use std::fmt::Write as _;
use std::path::Path;

use enshwick::abi::Side;
use enshwick::content::Guid;

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

/// What `enshwick entity` answers.
#[must_use]
pub fn entity(seen: &Seen) -> String {
    if seen.template.is_none() && seen.item.is_none() && seen.position.is_none() {
        return format!("entity {}: the server knows nothing of it", seen.entity);
    }
    let mut out = format!(
        "entity {}: template {}, item {}, at {}",
        seen.entity,
        seen.template
            .map_or_else(|| "-".to_owned(), |t| Guid(t).to_string()),
        seen.item.map_or_else(|| "-".to_owned(), |i| i.to_string()),
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

/// What `enshwick placed` answers for `placed`, newest first.
#[must_use]
pub fn placed(placed: &[Placed]) -> String {
    if placed.is_empty() {
        return "nothing placed since the server started".to_owned();
    }
    placed
        .iter()
        .map(|p| {
            format!(
                "entity {}: item {}, template {}, at {}",
                p.entity,
                p.item,
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
/// The word that is no number.
pub fn placed_count(args: &str) -> Result<usize, String> {
    let args = args.trim();
    if args.is_empty() {
        return Ok(REMEMBERED);
    }
    args.parse::<usize>()
        .map(|n| n.clamp(1, REMEMBERED))
        .map_err(|_| args.to_owned())
}

/// A loaded mod, for `enshwick mods`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Loaded<'a> {
    /// The mod id.
    pub id: &'a str,
    /// Its version.
    pub version: &'a str,
    /// Where it runs.
    pub side: Side,
    /// Whether players need it to join.
    pub join: bool,
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
            let side = match m.side {
                Side::SERVER => "server",
                Side::CLIENT => "client",
                _ => "both",
            };
            let join = if m.join { ", join" } else { "" };
            let state = if m.running { "running" } else { "off" };
            format!("{} {} ({side}{join}): {state}", m.id, m.version)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Makes mod `id` read its settings again; the reply.
///
/// While the world `world` runs, its own copy,
/// `mod-data/<id>/worlds/<world>/settings.toml`, is in force over
/// `mod-configs/<id>.toml`; that file otherwise. The file's modified time
/// moves to now, and the loader's watcher raises the mod's settings change
/// within a second.
#[must_use]
pub fn reload(game: &Path, id: &str, world: Option<&str>) -> String {
    let world_file = world
        .map(|key| format!("mod-data/{id}/worlds/{key}/settings.toml"))
        .filter(|file| game.join(file).is_file());
    let shown = world_file.unwrap_or_else(|| format!("mod-configs/{id}.toml"));
    let touched = std::fs::File::options()
        .append(true)
        .open(game.join(&shown))
        .and_then(|opened| opened.set_modified(std::time::SystemTime::now()));
    match touched {
        Ok(()) => format!("{id} reads {shown} again within a second"),
        Err(e) => format!("{id}: {shown}: {e}"),
    }
}

/// `x, y, z` in metres to one decimal, or `-`.
fn place(position: Option<[f64; 3]>) -> String {
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
            entity(&Seen {
                entity: 7,
                ..Seen::default()
            }),
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
            entity(&seen),
            "entity 12: template 6943053a-32c1-4718-ae22-e61427ddf54d, item 77, at 1.0, 2.0, 3.0, the character of Ann"
        );
        let piece = Placed {
            entity: 5,
            item: 9,
            template: [0; 16],
            position: [0.5, 1.0, 2.0],
        };
        assert_eq!(
            placed(&[piece]),
            "entity 5: item 9, template 00000000-0000-0000-0000-000000000000, at 0.5, 1.0, 2.0"
        );
        assert_eq!(placed(&[]), "nothing placed since the server started");
    }

    #[test]
    fn a_placed_count_stays_within_what_is_kept() {
        assert_eq!(placed_count(""), Ok(REMEMBERED));
        assert_eq!(placed_count(" 5 "), Ok(5));
        assert_eq!(placed_count("0"), Ok(1));
        assert_eq!(placed_count("999"), Ok(REMEMBERED));
        assert_eq!(placed_count("many"), Err("many".to_owned()));
    }

    #[test]
    fn mods_name_side_join_and_state() {
        assert_eq!(mods(&[]), "no mods loaded");
        let loaded = [
            Loaded {
                id: "you.trading-post",
                version: "0.3.0",
                side: Side::SERVER,
                join: true,
                running: true,
            },
            Loaded {
                id: "you.zones",
                version: "0.1.0",
                side: Side::CLIENT,
                join: false,
                running: false,
            },
        ];
        assert_eq!(
            mods(&loaded),
            "you.trading-post 0.3.0 (server, join): running\nyou.zones 0.1.0 (client): off"
        );
    }

    #[test]
    fn a_fixed_point_position_reads_in_metres() {
        assert_eq!(metres([1 << 32, -(3 << 31), 0]), [1.0, -1.5, 0.0]);
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn reload_touches_the_running_world_s_own_settings_or_else_the_template() {
        let game = std::env::temp_dir().join(format!("enshwick-reload-{}", std::process::id()));
        let world = game.join("mod-data/you.safes/worlds/world0-1a2b/settings.toml");
        let template = game.join("mod-configs/you.safes.toml");
        std::fs::create_dir_all(world.parent().unwrap()).unwrap();
        std::fs::create_dir_all(game.join("mod-configs")).unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        for path in [&world, &template] {
            std::fs::write(path, "enabled = true\n").unwrap();
            std::fs::File::options()
                .append(true)
                .open(path)
                .unwrap()
                .set_modified(old)
                .unwrap();
        }
        assert_eq!(
            reload(&game, "you.safes", Some("world0-1a2b")),
            "you.safes reads mod-data/you.safes/worlds/world0-1a2b/settings.toml again within a second"
        );
        assert!(std::fs::metadata(&world).unwrap().modified().unwrap() > old);
        assert_eq!(
            std::fs::metadata(&template).unwrap().modified().unwrap(),
            old,
            "the template stays"
        );
        assert_eq!(
            reload(&game, "you.safes", None),
            "you.safes reads mod-configs/you.safes.toml again within a second"
        );
        assert_eq!(
            std::fs::read_to_string(&template).unwrap(),
            "enabled = true\n"
        );
        assert!(reload(&game, "you.missing", None).starts_with("you.missing: "));
        std::fs::remove_dir_all(&game).unwrap();
    }
}
