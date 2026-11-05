//! An example mod with a panel at the right edge of the game's storage chest
//! screen: a line of text and a button that runs a player action, which the
//! server answers with a notice.

use enshwick::actions::{Action, Outcome};
use enshwick::i18n::Texts;
use enshwick::ui::{Edge, HudScreenType, Label, Style};
use enshwick::{Load, Mod, Off};

/// The mod's texts.
const TEXTS: &str = r#"
[en]
"panel" = "A panel of the chest-panel example"
"wave" = "Wave"
"waved" = "{name} waves from a chest."
[uk]
"panel" = "Панель прикладу chest-panel"
"wave" = "Помахати"
"waved" = "{name} махає від скрині."
"#;

/// The mod: everything happens in its handlers.
#[derive(Debug)]
pub struct ChestPanel;

impl Mod for ChestPanel {
    fn init(load: &mut Load<'_>) -> Result<Self, Off> {
        let actions = load.actions(Texts::parse(TEXTS)?)?;
        actions.add(Action::new("wave", "wave"), |request| {
            Outcome::done(request.say("waved", &[("name", &request.player.name)]))
        })?;
        if load.host().build().side != enshwick::abi::Side::CLIENT {
            return Ok(Self);
        }
        let texts = Texts::parse(TEXTS)?;
        let language = load.host().language();
        let say = move |key: &str| texts.get(key, language).unwrap_or(key).to_owned();
        let (line, wave) = (say("panel"), say("wave"));
        load.extend_screen(HudScreenType::StorageChest, Edge::Right, move |panel| {
            panel.label(Label::new(&line).style(Style::Secondary).wrap());
            if panel.button(wave.as_str())
                && let Err(why) = actions.run("wave", &[])
            {
                enshwick::warn!("the wave did not reach the server: {why}");
            }
        })?;
        Ok(Self)
    }
}

enshwick::declare_mod!(ChestPanel);

#[cfg(test)]
mod tests {
    use enshwick::abi::{Side, Status, UiExtension};
    use enshwick::test::{Drawn, Input};
    use enshwick::ui::HudScreenType;

    use super::*;

    fn start(side: Side) -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new().side(side).install();
        // SAFETY: the exports of this crate's `declare_mod!(ChestPanel)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    #[test]
    fn the_button_beside_a_storage_chest_asks_the_server_to_wave() {
        let loader = start(Side::CLIENT);
        loader.input("Wave", Input::Click);
        let drawn = loader.draw_screen(HudScreenType::StorageChest);
        assert_eq!(drawn.len(), 1);
        assert_eq!(drawn[0].0, UiExtension::RIGHT);
        assert!(matches!(&drawn[0].1[1], Drawn::Button { text, .. } if text == "Wave"));
        assert_eq!(loader.action_runs(), [("wave".to_owned(), String::new())]);
        assert!(loader.draw_screen(HudScreenType::Backpack).is_empty());
    }

    #[test]
    fn a_server_runs_the_wave_and_draws_nothing() {
        let loader = start(Side::SERVER);
        assert!(loader.draw_screen(HudScreenType::StorageChest).is_empty());
    }
}
