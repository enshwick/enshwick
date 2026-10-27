//! The console's orders on the world, read from a line's words: give items,
//! spawn from a template and teleport a player.
//!
//! A player's name may hold spaces, so it always comes last.

use enshwick::content::Guid;

/// The most items one `give` or `give-to` creates per player.
pub const MAX_ITEMS: u32 = 10_000;
/// The most entities one `spawn` creates.
pub const MAX_SPAWNS: u32 = 32;
/// The farthest a place may lie from the origin on any axis, in metres:
/// well past the edge of the world, well inside the game's fixed-point
/// range.
pub const MAX_METRES: f64 = 100_000.0;

/// The key of the usage of `give <item> <count>`: items for every player
/// online.
pub const GIVE_USAGE: &str = "usage.give";
/// The key of the usage of `give-to <item> <count> <player or account>`:
/// items for one player.
pub const GIVE_TO_USAGE: &str = "usage.give-to";
/// The key of the usage of `spawn <template> <count> <x> <y> <z> | <player
/// or account>`.
pub const SPAWN_USAGE: &str =
    "enshwick spawn <template GUID> <count> <x> <y> <z> | <player or account>";
/// The key of the usage of `teleport <x> <y> <z> <player or account>`.
pub const TELEPORT_USAGE: &str = "usage.teleport";

/// Who receives a give.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Recipients<'a> {
    /// Every player online.
    Everyone,
    /// The player or account the words name.
    Named(&'a str),
}

/// Items the console gives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Give<'a> {
    /// The item id, as `enshwick entity` shows it.
    pub item: u32,
    /// How many, 1 to [`MAX_ITEMS`].
    pub count: u32,
    /// Who gets them.
    pub to: Recipients<'a>,
}

/// Where a spawn or a teleport lands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place<'a> {
    /// A point in metres.
    Point([f64; 3]),
    /// Beside the player or account the words name.
    Beside(&'a str),
}

/// Entities the console spawns.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Spawn<'a> {
    /// The template GUID, in file order.
    pub template: [u8; 16],
    /// How many, 1 to [`MAX_SPAWNS`].
    pub count: u32,
    /// Where.
    pub at: Place<'a>,
}

/// A player the console teleports.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Teleport<'a> {
    /// Where to, in metres.
    pub to: [f64; 3],
    /// The player or account the words name.
    pub who: &'a str,
}

/// The line's words do not fit the tool; the answer shows its usage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Usage(pub &'static str);

/// An id as `enshwick entity` shows it, or hex after `0x`; never 0.
fn id(word: &str) -> Option<u32> {
    let parsed = match word.strip_prefix("0x").or_else(|| word.strip_prefix("0X")) {
        Some(hex) => u32::from_str_radix(hex, 16).ok(),
        None => word.parse().ok(),
    };
    parsed.filter(|&id| id != 0)
}

/// A count from 1 to `max`.
fn count(word: &str, max: u32) -> Option<u32> {
    word.parse().ok().filter(|count| (1..=max).contains(count))
}

/// A coordinate in metres, at most [`MAX_METRES`] from the origin.
fn metres(word: &str) -> Option<f64> {
    word.parse::<f64>().ok().filter(|m| m.abs() <= MAX_METRES)
}

/// The first `N` words of `args` and the rest, trimmed.
fn words<const N: usize>(args: &str) -> Option<([&str; N], &str)> {
    let mut rest = args.trim();
    let mut out = [""; N];
    for word in &mut out {
        let (first, after) = rest.split_once(char::is_whitespace).unwrap_or((rest, ""));
        if first.is_empty() {
            return None;
        }
        *word = first;
        rest = after.trim_start();
    }
    Some((out, rest.trim_end()))
}

fn point([x, y, z]: [&str; 3]) -> Option<[f64; 3]> {
    Some([metres(x)?, metres(y)?, metres(z)?])
}

/// `give <item> <count>`.
///
/// # Errors
///
/// [`Usage`] when the words do not fit.
pub fn give(args: &str) -> Result<Give<'_>, Usage> {
    let usage = Usage(GIVE_USAGE);
    let ([item, n], "") = words::<2>(args).ok_or(usage)? else {
        return Err(usage);
    };
    Ok(Give {
        item: id(item).ok_or(usage)?,
        count: count(n, MAX_ITEMS).ok_or(usage)?,
        to: Recipients::Everyone,
    })
}

/// `give-to <item> <count> <player or account>`.
///
/// # Errors
///
/// [`Usage`] when the words do not fit.
pub fn give_to(args: &str) -> Result<Give<'_>, Usage> {
    let usage = Usage(GIVE_TO_USAGE);
    let ([item, n], who) = words::<2>(args).ok_or(usage)?;
    if who.is_empty() {
        return Err(usage);
    }
    Ok(Give {
        item: id(item).ok_or(usage)?,
        count: count(n, MAX_ITEMS).ok_or(usage)?,
        to: Recipients::Named(who),
    })
}

/// `spawn <template> <count> <x> <y> <z>` or `spawn <template> <count>
/// <player or account>`; three numbers are always a point.
///
/// # Errors
///
/// [`Usage`] when the words do not fit.
pub fn spawn(args: &str) -> Result<Spawn<'_>, Usage> {
    let usage = Usage(SPAWN_USAGE);
    let ([template, n], rest) = words::<2>(args).ok_or(usage)?;
    let template = Guid::try_parse(template).ok_or(usage)?.0;
    if template == [0; 16] || rest.is_empty() {
        return Err(usage);
    }
    let at = words::<3>(rest)
        .filter(|(_, after)| after.is_empty())
        .and_then(|(xyz, _)| point(xyz))
        .map_or(Place::Beside(rest), Place::Point);
    Ok(Spawn {
        template,
        count: count(n, MAX_SPAWNS).ok_or(usage)?,
        at,
    })
}

/// The key of the usage of `teleport <x> <y> <z> <player or account>`.
///
/// # Errors
///
/// [`Usage`] when the words do not fit.
pub fn teleport(args: &str) -> Result<Teleport<'_>, Usage> {
    let usage = Usage(TELEPORT_USAGE);
    let (xyz, who) = words::<3>(args).ok_or(usage)?;
    if who.is_empty() {
        return Err(usage);
    }
    Ok(Teleport {
        to: point(xyz).ok_or(usage)?,
        who,
    })
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    const ALTAR: &str = "6943053a-32c1-4718-ae22-e61427ddf54d";

    #[test]
    fn give_gives_every_player_online_and_give_to_names_one_last() {
        assert_eq!(
            give("77 5"),
            Ok(Give {
                item: 77,
                count: 5,
                to: Recipients::Everyone
            })
        );
        assert_eq!(
            give_to(" 0x4d  2 Ann Lee "),
            Ok(Give {
                item: 0x4d,
                count: 2,
                to: Recipients::Named("Ann Lee")
            })
        );
        assert_eq!(give("77 5 Ann"), Err(Usage(GIVE_USAGE)));
        assert_eq!(give_to("77 5"), Err(Usage(GIVE_TO_USAGE)));
        assert_eq!(give("77"), Err(Usage(GIVE_USAGE)));
    }

    #[test]
    fn a_give_refuses_item_zero_and_counts_out_of_range() {
        assert_eq!(give("0 1"), Err(Usage(GIVE_USAGE)));
        assert_eq!(give("7 0"), Err(Usage(GIVE_USAGE)));
        assert_eq!(give("7 -1"), Err(Usage(GIVE_USAGE)));
        assert_eq!(
            give(&format!("7 {}", MAX_ITEMS + 1)),
            Err(Usage(GIVE_USAGE))
        );
        assert!(give(&format!("7 {MAX_ITEMS}")).is_ok());
        assert_eq!(give("seven 1"), Err(Usage(GIVE_USAGE)));
    }

    #[test]
    fn three_numbers_after_a_spawn_s_count_are_a_point_and_anything_else_a_player() {
        let template = Guid::parse(ALTAR).0;
        assert_eq!(
            spawn(&format!("{ALTAR} 3 1.5 -2 30")),
            Ok(Spawn {
                template,
                count: 3,
                at: Place::Point([1.5, -2.0, 30.0])
            })
        );
        assert_eq!(
            spawn(&format!("{ALTAR} 1 Ann Lee")),
            Ok(Spawn {
                template,
                count: 1,
                at: Place::Beside("Ann Lee")
            })
        );
        assert_eq!(
            spawn(&format!("{ALTAR} 1 1 2")),
            Ok(Spawn {
                template,
                count: 1,
                at: Place::Beside("1 2")
            })
        );
    }

    #[test]
    fn a_spawn_refuses_a_bad_template_count_or_no_place() {
        let usage = Err(Usage(SPAWN_USAGE));
        assert_eq!(spawn("6943053a 1 0 0 0"), usage);
        assert_eq!(spawn("00000000-0000-0000-0000-000000000000 1 0 0 0"), usage);
        assert_eq!(spawn(&format!("{ALTAR} 0 0 0 0")), usage);
        assert_eq!(spawn(&format!("{ALTAR} {} 0 0 0", MAX_SPAWNS + 1)), usage);
        assert_eq!(spawn(&format!("{ALTAR} 1")), usage);
    }

    #[test]
    fn a_teleport_takes_a_finite_point_then_the_player() {
        assert_eq!(
            teleport("10 20.5 -3 Ann Lee"),
            Ok(Teleport {
                to: [10.0, 20.5, -3.0],
                who: "Ann Lee"
            })
        );
        let usage = Err(Usage(TELEPORT_USAGE));
        assert_eq!(teleport("10 20 -3"), usage);
        assert_eq!(teleport("10 NaN 0 Ann"), usage);
        assert_eq!(teleport("10 inf 0 Ann"), usage);
        assert_eq!(teleport("1e9 0 0 Ann"), usage);
        assert_eq!(teleport("0 -100001 0 Ann"), usage);
        assert!(teleport(&format!("{MAX_METRES} 0 0 Ann")).is_ok());
        assert_eq!(teleport("Ann 1 2 3"), usage);
    }

    proptest! {
        #[test]
        fn no_line_makes_an_order_panic(line in "\\PC{0,80}") {
            let _ = (give(&line), give_to(&line), spawn(&line), teleport(&line));
        }

        #[test]
        fn a_teleport_reads_back_any_point_in_range(
            x in -MAX_METRES..=MAX_METRES,
            y in -MAX_METRES..=MAX_METRES,
            z in -MAX_METRES..=MAX_METRES,
        ) {
            let line = format!("{x} {y} {z} Ann");
            let order = teleport(&line).unwrap();
            prop_assert_eq!(order.to.map(f64::to_bits), [x, y, z].map(f64::to_bits));
            prop_assert_eq!(order.who, "Ann");
        }
    }
}
