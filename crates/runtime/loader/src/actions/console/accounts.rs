//! Kick, ban and unban by account: whether the game would act on an
//! account, and the account action its own handler takes.

/// What the game's account action handler does to an account; the values
/// of the game's account action type (server 387 and 399, clients 410 and
/// 426).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Act {
    /// Removes a player online from the session.
    Kick,
    /// Adds the account of a player online to the server's ban list, and
    /// kicks the player.
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

/// Whom an account action names.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    /// A player online.
    Player(Online),
    /// An account no player online holds.
    Account(u64),
}

/// Why the game is asked for nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// The player is an administrator, whom the game never kicks or bans.
    Admin(String),
    /// The account is not online, and the game kicks and bans only players
    /// online.
    Offline(u64),
}

/// The account `word` spells: `0x` and hex, or exactly the sixteen hex
/// digits `enshwick world` shows; 0 is no account.
#[must_use]
pub fn account(word: &str) -> Option<u64> {
    let prefixed = word.starts_with("0x") || word.starts_with("0X");
    if !prefixed && word.len() != 16 {
        return None;
    }
    enshwick_core::PlayerKey::from_hex(word)
        .map(enshwick_core::PlayerKey::get)
        .filter(|&key| key != 0)
}

/// The account the game is asked to `act` on for `target`.
///
/// # Errors
///
/// A [`Refusal`] for what the game would refuse without a word: an
/// administrator, or an account not online for a kick or a ban.
pub fn decide(act: Act, target: &Target) -> Result<u64, Refusal> {
    match target {
        Target::Player(player) if player.admin && act != Act::Unban => {
            Err(Refusal::Admin(shown(&player.name)))
        }
        Target::Player(player) => Ok(player.key),
        Target::Account(key) if act != Act::Unban => Err(Refusal::Offline(*key)),
        Target::Account(key) => Ok(*key),
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
        assert_eq!(decide(Act::Ban, &ann), Ok(0x2a));
        assert_eq!(decide(Act::Kick, &bob), Err(Refusal::Admin("Bob".into())));
        assert_eq!(decide(Act::Ban, &bob), Err(Refusal::Admin("Bob".into())));
        assert_eq!(decide(Act::Unban, &bob), Ok(0x11));
        assert_eq!(decide(Act::Kick, &away), Err(Refusal::Offline(0x99)));
        assert_eq!(decide(Act::Ban, &away), Err(Refusal::Offline(0x99)));
        assert_eq!(decide(Act::Unban, &away), Ok(0x99));
    }

    #[test]
    fn a_name_with_control_characters_stays_on_one_line() {
        let forged = Target::Player(player("Ann\n[WARN] forged", 1, true));
        let Err(Refusal::Admin(name)) = decide(Act::Kick, &forged) else {
            panic!("an administrator is refused");
        };
        assert!(!name.contains('\n'), "{name}");
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
