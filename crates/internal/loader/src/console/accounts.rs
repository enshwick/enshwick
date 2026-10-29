//! Kick, ban and unban by account: which account a console line names,
//! whether the game would act on it, and the account action its own
//! handler takes.

use std::fmt::Write as _;

/// What the game's account action handler does to an account; the values
/// of the game's account action type (server 387 and 399, clients 410 and
/// 426).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// Removes a player online from the session.
    Kick,
    /// Adds an account to the server's ban list, and kicks its player when
    /// online.
    Ban,
    /// Takes an account off the server's ban list.
    Unban,
}

impl Act {
    /// The value the game's account action carries.
    #[must_use]
    pub const fn code(self) -> u8 {
        match self {
            Self::Kick => 0,
            Self::Ban => 1,
            Self::Unban => 2,
        }
    }

    /// Whether the game acts only on a player online.
    #[must_use]
    pub const fn needs_player(self) -> bool {
        matches!(self, Self::Kick)
    }
}

/// A player online, as far as kicking goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Online {
    /// The name the client sent.
    pub name: String,
    /// The account.
    pub key: u64,
    /// Whether the player is an administrator, whom the game never kicks.
    pub admin: bool,
}

/// Whom a console line's words name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A player online.
    Player(Online),
    /// An account no player online holds.
    Account(u64),
}

/// Why the console asks the game for nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The line names nobody.
    Empty,
    /// No player online has the name, and it spells no account.
    Unknown(String),
    /// Several players online share the name: their accounts.
    Shared(String, Vec<u64>),
    /// The player is an administrator, whom the game never kicks or bans.
    Admin(String),
    /// The account is not online, and the game kicks only players online.
    Offline(u64),
}

/// How the game is asked for an account action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// Through its account action handler, as its administrators ask.
    Handler,
    /// Straight onto its ban list: the handler bans only players online.
    BanList,
}

/// What the game is asked for: the account, how an answer names it, and
/// the way to ask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    /// The account.
    pub key: u64,
    /// The player's name and account, or the account alone.
    pub who: String,
    /// How the game is asked.
    pub route: Route,
}

/// The account `word` spells: `0x` and hex, or exactly the sixteen hex
/// digits `enshwick world` shows; 0 is no account.
#[must_use]
pub fn account(word: &str) -> Option<u64> {
    let hex = match word.strip_prefix("0x").or_else(|| word.strip_prefix("0X")) {
        Some(hex) => hex,
        None if word.len() == 16 => word,
        None => return None,
    };
    if hex.is_empty() || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u64::from_str_radix(hex, 16).ok().filter(|&key| key != 0)
}

/// Whom `args` names among `online`: an account first, so a name never
/// takes what is meant for an account, then the one player online with
/// that name, in any case.
///
/// # Errors
///
/// A [`Refusal`] when `args` names nobody or a name several players share.
pub fn target(args: &str, online: &[Online]) -> Result<Target, Refusal> {
    let args = args.trim();
    if args.is_empty() {
        return Err(Refusal::Empty);
    }
    if let Some(key) = account(args) {
        return Ok(online
            .iter()
            .find(|p| p.key == key)
            .map_or(Target::Account(key), |p| Target::Player(p.clone())));
    }
    let wanted = args.to_lowercase();
    let named: Vec<&Online> = online
        .iter()
        .filter(|p| p.name.to_lowercase() == wanted)
        .collect();
    match named.as_slice() {
        [] => Err(Refusal::Unknown(shown(args))),
        [player] => Ok(Target::Player((*player).clone())),
        many => Err(Refusal::Shared(
            shown(args),
            many.iter().map(|p| p.key).collect(),
        )),
    }
}

/// What the game is asked to `act` on for `target`.
///
/// # Errors
///
/// A [`Refusal`] for what the game would refuse without a word: an
/// administrator, or an account not online for a kick.
pub fn decide(act: Act, target: &Target) -> Result<Decision, Refusal> {
    match target {
        Target::Player(player) if player.admin && act != Act::Unban => {
            Err(Refusal::Admin(shown(&player.name)))
        }
        Target::Player(player) => Ok(Decision {
            key: player.key,
            who: format!("{} ({:016x})", shown(&player.name), player.key),
            route: Route::Handler,
        }),
        Target::Account(key) if act.needs_player() => Err(Refusal::Offline(*key)),
        Target::Account(key) => Ok(Decision {
            key: *key,
            who: format!("{key:016x}"),
            route: if act == Act::Ban {
                Route::BanList
            } else {
                Route::Handler
            },
        }),
    }
}

/// A name a client sent, safe for one log or console line: control
/// characters become `?`.
#[must_use]
pub fn shown(name: &str) -> String {
    name.chars()
        .map(|c| if c.is_control() { '?' } else { c })
        .collect()
}

/// Accounts as `enshwick world` shows them, joined by commas.
#[must_use]
pub fn listed(keys: &[u64]) -> String {
    let mut out = String::new();
    for (i, key) in keys.iter().enumerate() {
        let comma = if i == 0 { "" } else { ", " };
        let _w = write!(out, "{comma}{key:016x}");
    }
    out
}

/// The game's account action event for `act` on `account`, as 8-byte words
/// of the reflected `size`, with the account's `u64` at `account_at` and
/// the action's byte at `act_at`; `None` when a field falls outside.
#[must_use]
pub fn event(
    size: usize,
    account_at: usize,
    act_at: usize,
    act: Act,
    account: u64,
) -> Option<Vec<u64>> {
    let account_end = account_at.checked_add(8).filter(|&end| end <= size)?;
    if act_at >= size {
        return None;
    }
    let mut bytes = vec![0u8; size.div_ceil(8) * 8];
    bytes[account_at..account_end].copy_from_slice(&account.to_le_bytes());
    bytes[act_at] = act.code();
    let (words, _) = bytes.as_chunks::<8>();
    Some(words.iter().copied().map(u64::from_le_bytes).collect())
}

/// Where the game's ban entry in its live list keeps its fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BanFields {
    /// The entry's size.
    pub size: usize,
    /// `accountId`, a `u64`.
    pub account: usize,
    /// `displayName`, a string.
    pub display_name: usize,
    /// `characterName`, a string.
    pub character_name: usize,
    /// `banDate`, Unix seconds as an `i64`.
    pub date: usize,
    /// Where each name, a string with an inline buffer, keeps its text
    /// pointer, from the name's start.
    pub name_text: usize,
    /// Where each name keeps its length in bytes, from the name's start.
    pub name_length: usize,
}

/// The game's ban entry for `account`, banned at `date` in Unix seconds,
/// as 8-byte words: both names empty, their text pointer `empty`. `None`
/// when a field is not 8-aligned or falls outside the entry.
#[must_use]
pub fn ban_entry(fields: &BanFields, account: u64, date: i64, empty: u64) -> Option<Vec<u64>> {
    let word =
        |at: usize| (at.is_multiple_of(8) && at.checked_add(8)? <= fields.size).then_some(at / 8);
    let mut words = vec![0u64; fields.size.div_ceil(8)];
    let mut put = |at: usize, value: u64| -> Option<()> {
        *words.get_mut(word(at)?)? = value;
        Some(())
    };
    put(fields.account, account)?;
    for name in [fields.display_name, fields.character_name] {
        put(name.checked_add(fields.name_text)?, empty)?;
        put(name.checked_add(fields.name_length)?, 0)?;
    }
    put(fields.date, u64::from_ne_bytes(date.to_ne_bytes()))?;
    Some(words)
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    fn player(name: &str, key: u64, admin: bool) -> Online {
        Online {
            name: name.into(),
            key,
            admin,
        }
    }

    #[test]
    fn a_word_names_an_account_first_then_the_one_player_of_that_name() {
        let online = [player("Ann", 0x2a, false), player("Bob", 0x11, true)];
        assert_eq!(
            target("ann", &online),
            Ok(Target::Player(online[0].clone()))
        );
        assert_eq!(
            target("0000000000000011", &online),
            Ok(Target::Player(online[1].clone()))
        );
        assert_eq!(
            target("0x2A", &online),
            Ok(Target::Player(online[0].clone()))
        );
        assert_eq!(target("0x99", &online), Ok(Target::Account(0x99)));
        assert_eq!(
            target("Nobody", &online),
            Err(Refusal::Unknown("Nobody".into()))
        );
        assert_eq!(target("  ", &online), Err(Refusal::Empty));
    }

    #[test]
    fn a_name_several_players_share_names_nobody() {
        let online = [player("Ann", 1, false), player("ANN", 2, false)];
        assert_eq!(
            target("ann", &online),
            Err(Refusal::Shared("ann".into(), vec![1, 2]))
        );
        assert_eq!(listed(&[1, 2]), "0000000000000001, 0000000000000002");
    }

    #[test]
    fn a_player_named_like_an_account_never_takes_that_account_s_ban() {
        let online = [
            player("0000000000000099", 0x2a, false),
            player("Bob", 0x99, false),
        ];
        assert_eq!(
            target("0000000000000099", &online),
            Ok(Target::Player(online[1].clone()))
        );
        let alone = [player("0000000000000099", 0x2a, false)];
        assert_eq!(
            target("0000000000000099", &alone),
            Ok(Target::Account(0x99))
        );
    }

    #[test]
    fn decimal_and_short_numbers_spell_no_account() {
        assert_eq!(account("76561198000000000"), None);
        assert_eq!(account("1234"), None);
        assert_eq!(account("0x"), None);
        assert_eq!(account("0x+1"), None);
        assert_eq!(account("1234567890123456"), Some(0x1234_5678_9012_3456));
        assert_eq!(account("0x0"), None);
        assert_eq!(account("0000000000000000"), None);
    }

    #[test]
    fn the_game_is_asked_only_what_it_would_do() {
        let ann = Target::Player(player("Ann", 0x2a, false));
        let bob = Target::Player(player("Bob", 0x11, true));
        let away = Target::Account(0x99);
        let decision = |key, who: &str, route| Decision {
            key,
            who: who.to_owned(),
            route,
        };
        assert_eq!(
            decide(Act::Ban, &ann),
            Ok(decision(0x2a, "Ann (000000000000002a)", Route::Handler))
        );
        assert_eq!(decide(Act::Kick, &bob), Err(Refusal::Admin("Bob".into())));
        assert_eq!(decide(Act::Ban, &bob), Err(Refusal::Admin("Bob".into())));
        assert!(decide(Act::Unban, &bob).is_ok());
        assert_eq!(decide(Act::Kick, &away), Err(Refusal::Offline(0x99)));
        assert_eq!(
            decide(Act::Ban, &away),
            Ok(decision(0x99, "0000000000000099", Route::BanList))
        );
        assert_eq!(
            decide(Act::Unban, &away),
            Ok(decision(0x99, "0000000000000099", Route::Handler))
        );
    }

    #[test]
    fn a_name_with_control_characters_stays_on_one_line() {
        let forged = Target::Player(player("Ann\n[WARN] forged", 1, false));
        let who = decide(Act::Kick, &forged).unwrap().who;
        assert!(!who.contains('\n'), "{who}");
    }

    #[test]
    fn the_event_carries_the_account_and_the_act_where_reflection_puts_them() {
        let words = event(24, 8, 0x14, Act::Ban, 0x0102_0304_0506_0708).unwrap();
        assert_eq!(words, [0, 0x0102_0304_0506_0708, 0x0000_0001_0000_0000]);
        assert_eq!(event(24, 8, 0x14, Act::Kick, 1).unwrap()[2], 0);
        assert_eq!(event(24, 8, 0x14, Act::Unban, 1).unwrap()[2], 2 << 32);
        assert_eq!(event(24, 20, 0x14, Act::Kick, 1), None);
        assert_eq!(event(24, 8, 24, Act::Kick, 1), None);
        assert_eq!(event(24, usize::MAX, 0, Act::Kick, 1), None);
    }

    #[test]
    fn only_a_kick_needs_the_player_online() {
        assert!(Act::Kick.needs_player());
        assert!(!Act::Ban.needs_player());
        assert!(!Act::Unban.needs_player());
    }

    const LIVE: BanFields = BanFields {
        size: 0xd0,
        account: 0,
        display_name: 8,
        character_name: 0x68,
        date: 0xc8,
        name_text: 0,
        name_length: 8,
    };

    #[test]
    fn a_ban_entry_holds_the_account_the_date_and_two_empty_names() {
        let fields = LIVE;
        let words = ban_entry(&fields, 0x0102_0304_0506_0708, 1_761_500_000, 0xabc0).unwrap();
        assert_eq!(words.len(), 0xd0 / 8);
        assert_eq!(words[0], 0x0102_0304_0506_0708);
        assert_eq!(words[1], 0xabc0);
        assert_eq!(words[2], 0);
        assert_eq!(words[0x68 / 8], 0xabc0);
        assert_eq!(words[0x68 / 8 + 1], 0);
        assert_eq!(words[0xc8 / 8], 1_761_500_000);
        assert_eq!(words.iter().filter(|&&w| w != 0).count(), 4);
    }

    #[test]
    fn a_ban_entry_puts_each_name_s_text_and_length_where_the_build_keeps_them() {
        let fields = BanFields {
            name_text: 8,
            name_length: 0,
            ..LIVE
        };
        let words = ban_entry(&fields, 7, 0, 0xabc0).unwrap();
        assert_eq!(words[1], 0);
        assert_eq!(words[2], 0xabc0);
        assert_eq!(words[0x68 / 8], 0);
        assert_eq!(words[0x68 / 8 + 1], 0xabc0);
    }

    #[test]
    fn a_ban_entry_whose_fields_fall_outside_is_none() {
        let fields = LIVE;
        assert!(
            ban_entry(
                &BanFields {
                    name_length: 0x10,
                    character_name: 0xc0,
                    ..fields
                },
                1,
                0,
                0
            )
            .is_none()
        );
        assert!(
            ban_entry(
                &BanFields {
                    size: 0xc8,
                    ..fields
                },
                1,
                0,
                0
            )
            .is_none()
        );
        assert!(
            ban_entry(
                &BanFields {
                    character_name: 0xc4,
                    ..fields
                },
                1,
                0,
                0
            )
            .is_none()
        );
        assert!(
            ban_entry(
                &BanFields {
                    account: 3,
                    ..fields
                },
                1,
                0,
                0
            )
            .is_none()
        );
        assert!(
            ban_entry(
                &BanFields {
                    date: usize::MAX,
                    ..fields
                },
                1,
                0,
                0
            )
            .is_none()
        );
    }

    proptest! {
        #[test]
        fn an_account_as_world_shows_it_reads_back(key in 1u64..) {
            prop_assert_eq!(account(&format!("{key:016x}")), Some(key));
            prop_assert_eq!(account(&format!("0x{key:x}")), Some(key));
        }

        #[test]
        fn an_event_never_panics_and_puts_its_fields_where_asked(
            size in 0usize..256,
            account_at in 0usize..264,
            act_at in 0usize..264,
            key: u64,
        ) {
            let built = event(size, account_at, act_at, Act::Unban, key);
            prop_assert_eq!(built.is_some(), account_at + 8 <= size && act_at < size);
            if let Some(words) = built {
                let bytes: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
                prop_assert!(bytes.len() >= size && bytes.len().is_multiple_of(8));
                prop_assert_eq!(bytes[act_at], Act::Unban.code());
                if !(account_at..account_at + 8).contains(&act_at) {
                    prop_assert_eq!(&bytes[account_at..account_at + 8], &key.to_le_bytes());
                }
            }
        }
    }
}
