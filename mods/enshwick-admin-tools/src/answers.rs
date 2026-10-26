//! What the administrator actions answer.

use std::fmt::Write as _;
use std::path::Path;

use enshwick::content::Guid;

/// The most items one `give` hands out.
pub const MOST: u32 = 10_000;

/// A player online, for `world`.
#[derive(Clone, Debug, PartialEq)]
pub struct Online {
    /// The name the client sent.
    pub name: String,
    /// The server's slot.
    pub slot: u32,
    /// The character's entity.
    pub entity: u32,
    /// Whether the player is an administrator.
    pub admin: bool,
    /// Where the character stands, in metres.
    pub position: Option<[f64; 3]>,
}

/// What `world` answers.
///
/// # Examples
///
/// ```
/// use enshwick_admin_tools::answers::{Online, world};
///
/// let ann = Online { name: "Ann".into(), slot: 0, entity: 9, admin: true, position: Some([1.0, 2.5, -3.0]) };
/// assert_eq!(world(&[ann]), "1 player online\nAnn (slot 0, entity 9, admin) at 1.0, 2.5, -3.0");
/// ```
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
            "\n{} (slot {}, entity {}{admin}) at {}",
            p.name,
            p.slot,
            p.entity,
            place(p.position)
        );
    }
    out
}

/// What the server knows of an entity, for `entity`.
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

/// What `entity` answers.
///
/// # Examples
///
/// ```
/// use enshwick_admin_tools::answers::{Seen, entity};
///
/// assert_eq!(entity(&Seen { entity: 7, ..Seen::default() }), "entity 7: the server knows nothing of it");
/// ```
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

/// A piece a player placed, for `placed`.
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

/// What `placed` answers for `placed`, newest first.
///
/// # Examples
///
/// ```
/// use enshwick_admin_tools::answers::placed;
///
/// assert_eq!(placed(&[]), "nothing placed since the world started");
/// ```
#[must_use]
pub fn placed(placed: &[Placed]) -> String {
    if placed.is_empty() {
        return "nothing placed since the world started".to_owned();
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

/// What `mods` answers.
///
/// # Examples
///
/// ```
/// use enshwick_admin_tools::answers::mods;
///
/// assert_eq!(mods(&[]), "no mods listed");
/// ```
#[must_use]
pub fn mods(entries: &[enshwick::mods::ModEntry]) -> String {
    if entries.is_empty() {
        return "no mods listed".to_owned();
    }
    entries
        .iter()
        .map(|m| {
            let side = match m.side {
                enshwick::abi::Side::SERVER => "server",
                enshwick::abi::Side::CLIENT => "client",
                _ => "both",
            };
            let join = if m.join { ", join" } else { "" };
            let state = if m.running { "running" } else { "off" };
            format!("{} {} ({side}{join}): {state}", m.id, m.version)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Makes mod `id` read its settings again: while the world `world` runs,
/// its own copy, `mod-data/<id>/worlds/<world>/settings.toml`, which is in
/// force over `mod-configs/<id>.toml`; that file otherwise. The file's
/// modified time moves to now, and the loader raises the mod's settings
/// change within a second. The reply.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replies_name_what_the_server_knows() {
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
        assert_eq!(world(&[]), "no players online");
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn reload_moves_the_settings_files_time() {
        let game = std::env::temp_dir().join(format!("admin-tools-reload-{}", std::process::id()));
        std::fs::create_dir_all(game.join("mod-configs")).unwrap();
        let path = game.join("mod-configs").join("safes.toml");
        std::fs::write(&path, "enabled = true\n").unwrap();
        let old = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        std::fs::File::options()
            .append(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
        assert_eq!(
            reload(&game, "safes", None),
            "safes reads mod-configs/safes.toml again within a second"
        );
        assert!(std::fs::metadata(&path).unwrap().modified().unwrap() > old);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "enabled = true\n");
        assert!(reload(&game, "missing", None).starts_with("missing: "));
        std::fs::remove_dir_all(&game).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore = "Miri isolates the file system")]
    fn reload_touches_the_running_world_s_own_settings() {
        let game = std::env::temp_dir().join(format!("admin-tools-world-{}", std::process::id()));
        let world = game.join("mod-data/safes/worlds/world0-1a2b/settings.toml");
        let template = game.join("mod-configs/safes.toml");
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
            reload(&game, "safes", Some("world0-1a2b")),
            "safes reads mod-data/safes/worlds/world0-1a2b/settings.toml again within a second"
        );
        assert!(std::fs::metadata(&world).unwrap().modified().unwrap() > old);
        let kept = std::fs::metadata(&template).unwrap().modified().unwrap();
        assert_eq!(kept, old, "the template stays");
        assert!(
            reload(&game, "safes", Some("other")).contains("mod-configs/safes.toml"),
            "a world without its own copy reads the template"
        );
        std::fs::remove_dir_all(&game).unwrap();
    }
}
