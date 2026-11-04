//! An example mod with two controls: G honks, through a player action the
//! server runs and answers with a notice, and a control linked to the
//! game's interact key logs, on the server, who interacted.

use enshwick::actions::{Action, Outcome};
use enshwick::controls::{Control, GameAction, Key};
use enshwick::i18n::Texts;
use enshwick::{Load, Mod, Off};

/// The mod's texts.
const TEXTS: &str = r#"
[en]
"horn" = "Sound the horn"
"interact" = "Note an interaction"
"honk" = "{name} honks."
[uk]
"horn" = "Посигналити"
"interact" = "Відзначити взаємодію"
"honk" = "{name} сигналить."
"#;

/// The mod: everything happens in its handlers.
#[derive(Debug)]
pub struct Horn;

impl Mod for Horn {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let texts = Texts::parse(TEXTS)?;
        let language = load.host().language();
        let label = |key: &str| texts.get(key, language).unwrap_or(key).to_owned();
        let actions = load.actions(Texts::parse(TEXTS)?)?;
        actions.add(Action::new("honk", "horn"), |request| {
            enshwick::info!("{} honked", request.player.name);
            Outcome::done(request.say("honk", &[("name", &request.player.name)]))
        })?;
        load.control(
            Control::new("horn", &label("horn")).key(Key::parse("G")?),
            move |_| {
                if let Err(why) = actions.run("honk", &[]) {
                    enshwick::warn!("the horn did not reach the server: {why}");
                }
            },
        )?;
        load.control(
            Control::new("interact", &label("interact")).link(GameAction::ContextualAction),
            |fired| {
                if let Some(player) = &fired.player {
                    enshwick::info!("{} interacted", player.name);
                }
            },
        )?;
        Ok(Self)
    }
}

enshwick::declare_mod!(Horn);

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use enshwick::abi::{Header, PlayerInfo, Status};
    use enshwick::controls::{GameAction, Key};

    use super::*;

    fn start(host: enshwick::test::FakeHost) -> enshwick::test::Installed {
        let loader = host.install();
        // SAFETY: the exports of this crate's `declare_mod!(Horn)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn g_asks_the_server_to_honk_and_interacting_is_logged_for_the_player() {
        let loader = start(enshwick::test::FakeHost::new());
        let g = Key::parse("G").unwrap();
        assert_eq!(loader.keys(&[g], &[], Duration::from_millis(16)), 1);
        assert_eq!(loader.action_runs(), [("honk".to_owned(), String::new())]);
        let mut ann = PlayerInfo {
            header: Header::of::<PlayerInfo>(),
            slot: 0,
            entity: enshwick::Entity(2),
            key: enshwick::PlayerKey::new(7),
            permissions: 0,
            name_len: 3,
            name: [0; 64],
        };
        ann.name[..3].copy_from_slice(b"Ann");
        assert_eq!(loader.game_action(&ann, GameAction::ContextualAction), 1);
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("Ann interacted")),
            "{:?}",
            loader.logs()
        );
    }
}
