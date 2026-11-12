//! Controls as a script declares them: the namespace `controls`. A key
//! crosses as its text, such as `Shift+E`, a game action as its game name,
//! and a control as a table of the builder's fields.

use core::time::Duration;

use super::{Control, Fired, GameAction, Key, Phase};
use crate::script::{Bound, Error, FromScript, Function, IntoScript, Ty, Typed, Value};
use crate::world::Player;
use crate::{Load, Off};

impl Typed for Key {
    const TY: &'static Ty = &Ty::String;
}

impl IntoScript for Key {
    fn into_value(self) -> Value {
        Value::String(self.to_string())
    }
}

impl FromScript for Key {
    fn from_value(value: Value) -> Result<Self, Error> {
        Self::parse(&String::from_value(value)?).map_err(|e| Error::new(e.to_string()))
    }
}

impl Typed for GameAction {
    const TY: &'static Ty = &Ty::String;
}

impl IntoScript for GameAction {
    fn into_value(self) -> Value {
        Value::String(self.name().to_owned())
    }
}

impl FromScript for GameAction {
    fn from_value(value: Value) -> Result<Self, Error> {
        let name = String::from_value(value)?;
        Self::ALL
            .iter()
            .copied()
            .find(|action| action.name() == name)
            .ok_or_else(|| Error::new(format!("no game action {name}")))
    }
}

/// A control as a script declares it: the fields of [`Control`]'s builder.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "Control")]
pub(crate) struct ScriptControl {
    /// The control's name, 1 to 32 of `[a-z0-9-]`, as [`Control::new`] takes it.
    pub name: String,
    /// What the mod's key bindings show, a text key.
    pub label: String,
    /// The default key, as [`Control::key`] sets it.
    pub key: Option<Key>,
    /// A second default key, as [`Control::second`] sets it.
    pub second: Option<Key>,
    /// The game action it follows on the server, by its game name, as
    /// [`Control::link`] sets it.
    pub link: Option<GameAction>,
    /// The phases it fires on, as [`Control::fires`] sets them; empty is a
    /// press alone.
    #[script(default)]
    pub fires: Vec<Phase>,
    /// Seconds the key stays down before a hold fires, as
    /// [`Control::hold_for`] sets it.
    pub hold_for: Option<Duration>,
    /// The id of the mod's action the server runs each time the control
    /// fires on a client, as [`Control::runs`] sets it.
    pub runs: Option<String>,
}

impl From<ScriptControl> for Control {
    fn from(script: ScriptControl) -> Self {
        let mut control = Self::new(&script.name, &script.label).fires(&script.fires);
        control.keys = [script.key, script.second];
        control.link = script.link;
        control.action = script.runs;
        if let Some(hold) = script.hold_for {
            control = control.hold_for(hold);
        }
        control
    }
}

/// A control that fired, as a script's handler gets it: [`Fired`]'s fields.
#[derive(Clone, Debug, crate::Script)]
#[script(name = "Fired")]
pub(crate) struct ScriptFired {
    /// The control's name.
    pub control: String,
    /// What happened to it.
    pub phase: Phase,
    /// On the server, the player whose game action fired a linked control.
    pub player: Option<Player>,
}

impl From<&Fired<'_>> for ScriptFired {
    fn from(fired: &Fired<'_>) -> Self {
        Self {
            control: fired.control.to_owned(),
            phase: fired.phase,
            player: fired.player.clone(),
        }
    }
}

/// The functions of [`Key`] as a script's namespace `controls`, on a key
/// as its text.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct ControlFns;

#[crate::script(control_fns)]
#[expect(
    clippy::unused_self,
    reason = "a script call takes its namespace, though a key's function reads nothing of it"
)]
impl ControlFns {
    /// The key `text` names, in its own spelling, as [`Key::parse`] reads it.
    ///
    /// # Errors
    ///
    /// As [`Key::parse`].
    #[script]
    pub fn parse(self, text: &str) -> Result<Key, Off> {
        Ok(Key::parse(text)?)
    }

    /// The key `bits` hold, as [`Key::from_bits`] reads them; nil for bits no
    /// key can be bound to.
    #[script(name = "from_bits")]
    #[must_use]
    pub(crate) fn key_of_bits_script(self, bits: u32) -> Option<Key> {
        Key::from_bits(bits)
    }

    /// The key's scan code and modifiers together, as [`Key::bits`] gives them.
    #[script]
    #[must_use]
    pub const fn bits(self, key: Key) -> u32 {
        key.bits()
    }

    /// The key's scan code without modifiers, as [`Key::code`] gives it.
    #[script]
    #[must_use]
    pub const fn code(self, key: Key) -> u32 {
        key.code()
    }

    /// The modifiers' bits, 0 for none, as [`Key::modifiers`] gives them.
    #[script]
    #[must_use]
    pub const fn modifiers(self, key: Key) -> u32 {
        key.modifiers()
    }

    /// Whether the key is a mouse button, as [`Key::is_mouse`] says.
    #[script]
    #[must_use]
    pub const fn is_mouse(self, key: Key) -> bool {
        key.is_mouse()
    }
}

impl Bound for ControlFns {
    const NAME: &'static str = "controls";
    const DOC: &'static str =
        "Keys the mod declares for its players, and game actions they follow.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_CONTROL_FNS];

    fn resolve(_load: &mut impl crate::Loading) -> Result<Self, crate::Off> {
        Ok(Self)
    }
}

#[crate::script(controls)]
impl Load<'_> {
    /// Declares `control` with its handler `on`, as [`Load::control`] does.
    ///
    /// # Errors
    ///
    /// As [`Load::control`].
    #[script(name = "control")]
    pub(crate) fn control_script(
        &mut self,
        control: ScriptControl,
        on: impl Fn(ScriptFired) + Send + Sync + 'static,
    ) -> Result<(), Off> {
        self.control(control.into(), move |fired| on(fired.into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::abi::ControlSpec;

    #[test]
    fn a_script_s_control_names_its_keys_game_action_phases_and_hold() {
        let control = Control::from(ScriptControl {
            name: "lights".into(),
            label: "Lights".into(),
            key: Some(Key::parse("Shift+L").unwrap()),
            second: Some(Key::parse("Mouse4").unwrap()),
            link: Some(GameAction::ContextualActionTap),
            fires: vec![Phase::Hold, Phase::Release],
            hold_for: Some(Duration::from_millis(250)),
            runs: Some("lights".into()),
        });
        assert_eq!(
            control.keys,
            [Key::parse("Shift+L").ok(), Key::parse("Mouse4").ok()]
        );
        assert_eq!(control.link, Some(GameAction::ContextualActionTap));
        assert_eq!(control.fires, ControlSpec::HOLD | ControlSpec::RELEASE);
        assert_eq!(control.hold, Duration::from_millis(250));
        assert_eq!(control.action.as_deref(), Some("lights"));
        let plain = Control::from(ScriptControl {
            name: "x".into(),
            label: "X".into(),
            key: None,
            second: None,
            link: None,
            fires: Vec::new(),
            hold_for: None,
            runs: None,
        });
        assert_eq!(
            (
                plain.keys,
                plain.link,
                plain.fires,
                plain.hold,
                plain.action
            ),
            (
                [None, None],
                None,
                ControlSpec::PRESS,
                super::super::HOLD,
                None
            )
        );
    }

    #[test]
    fn a_key_and_a_game_action_cross_as_their_names() {
        let key = Key::parse("ctrl + f5").unwrap();
        assert!(matches!(key.into_value(), Value::String(s) if s == "Ctrl+F5"));
        assert!(Key::from_value(Value::String("Escape".into())).is_err());
        let action = GameAction::from_value(Value::String("ContextualAction_Tap".into()));
        assert_eq!(action.ok(), Some(GameAction::ContextualActionTap));
        assert!(GameAction::from_value(Value::String("Fly".into())).is_err());
    }
}
