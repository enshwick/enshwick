//! The keys the player bound to the game's own controls, read and rebound
//! on a client.

use crate::abi::{GameBinding, Status, Str};
use crate::controls::Key;
use crate::typed::input_mapping::runtime::GameplayAction;
use crate::{Host, Off};

use super::Input;

/// One key of a game control's keyboard and mouse binding, from
/// [`Input::binding`].
///
/// # Examples
///
/// ```
/// use enshwick::controls::Key;
/// use enshwick::input::BoundKey;
///
/// let interact = BoundKey::Key(Key::parse("E")?);
/// assert_eq!(interact.to_string(), "E");
/// assert_eq!(BoundKey::Reserved("LeftShift").to_string(), "LeftShift");
/// # Ok::<(), enshwick::controls::ParseKeyError>(())
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum BoundKey {
    /// A key or mouse button with its modifiers, one a mod control may
    /// take too.
    Key(Key),
    /// A key no mod control may take, by its US name: Escape, a Windows
    /// key or a modifier alone, such as `LeftShift` for the game's sprint.
    Reserved(&'static str),
    /// Input a [`Key`] does not name, such as the mouse wheel or a key
    /// without a US name.
    Other,
}

impl BoundKey {
    /// The name of [`BoundKey::Other`] in a script.
    const OTHER: &'static str = "Other";

    /// The key the loader's key form `bits` holds; `None` for no key.
    fn from_bits(bits: u32) -> Option<Self> {
        match bits {
            0 => None,
            GameBinding::OTHER => Some(Self::Other),
            bits => Some(Key::from_bits(bits).map_or_else(
                || crate::controls::reserved(bits).map_or(Self::Other, Self::Reserved),
                Self::Key,
            )),
        }
    }
}

impl core::fmt::Display for BoundKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Key(key) => key.fmt(f),
            Self::Reserved(name) => f.write_str(name),
            Self::Other => f.write_str(Self::OTHER),
        }
    }
}

impl crate::script::Typed for BoundKey {
    const TY: &'static crate::script::Ty = &crate::script::Ty::String;
}

impl crate::script::IntoScript for BoundKey {
    fn into_value(self) -> crate::script::Value {
        crate::script::Value::String(self.to_string())
    }
}

impl crate::script::FromScript for BoundKey {
    fn from_value(value: crate::script::Value) -> Result<Self, crate::script::Error> {
        let text = String::from_value(value)?;
        if text == Self::OTHER {
            return Ok(Self::Other);
        }
        match Key::parse(&text) {
            Ok(key) => Ok(Self::Key(key)),
            Err(why) => crate::controls::reserved_named(&text)
                .map(Self::Reserved)
                .ok_or_else(|| crate::script::Error::new(why.to_string())),
        }
    }
}

/// The keys the player's keyboard and mouse bindings give one of the game's
/// controls, as the game's input and key glyphs use them, a mod's rebind
/// included.
///
/// # Examples
///
/// See [`Input::binding`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub struct Binding {
    /// The first key, `None` for none.
    pub first: Option<BoundKey>,
    /// The second key, `None` for none.
    pub second: Option<BoundKey>,
}

/// Why the game's key bindings did not read or take a rebind.
///
/// # Examples
///
/// See [`Input::binding`].
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum BindingError {
    /// On a server, before the client's interface drew its first frame,
    /// on a game build where the loader does not find the bindings, or
    /// while they are off after a fault.
    #[error("the game's key bindings do not read here")]
    Unavailable,
    /// The running game build has no such action.
    #[error("this game build has no action {0}")]
    NoAction(String),
    /// The game's bindings do not take this key: one of its modifiers is
    /// not in the game's list.
    #[error("the game's key bindings do not take {0}")]
    Refused(Key),
}

impl From<BindingError> for Off {
    fn from(why: BindingError) -> Self {
        Self::new(why.to_string())
    }
}

/// The game's name of `action`, as the loader looks it up on the build.
fn name(action: &GameplayAction) -> Result<String, BindingError> {
    use crate::typed::Reflected as _;
    match action.to_value() {
        crate::resources::Value::Name(name) => Ok(name),
        _ => Err(BindingError::NoAction(format!("{action:?}"))),
    }
}

/// The loader's answer to a read or a rebind of `action`, `key` its key.
fn answered(status: Status, action: String, key: Option<Key>) -> Result<(), BindingError> {
    match status {
        Status::OK => Ok(()),
        Status::MISSING => Err(BindingError::NoAction(action)),
        Status::BAD_ARGS => key.map_or(Err(BindingError::Unavailable), |key| {
            Err(BindingError::Refused(key))
        }),
        _ => Err(BindingError::Unavailable),
    }
}

/// Writes `key`, or with 0 the game's own binding back, as `action`'s first
/// key.
fn write(host: Host, action: &GameplayAction, key: Option<Key>) -> Result<(), BindingError> {
    let action = name(action)?;
    let bits = key.map_or(0, Key::bits);
    let status = (host.api().game_rebind)(host.handle(), Str::new(&action), bits);
    answered(status, action, key)
}

#[crate::script(bindings)]
impl Input {
    /// The keys the player's keyboard and mouse bindings give `action`, a
    /// mod's rebind included. Client only, on any thread; the interface
    /// reads the bindings every few frames, so a change the player applies
    /// shows within a quarter second.
    ///
    /// # Errors
    ///
    /// [`BindingError::Unavailable`] on a server, before the interface drew
    /// its first frame and on a game build without the bindings;
    /// [`BindingError::NoAction`] for an action the build does not name.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::typed::input_mapping::runtime::GameplayAction;
    ///
    /// fn interact_key(input: enshwick::input::Input) -> Result<String, enshwick::input::BindingError> {
    ///     let binding = input.binding(&GameplayAction::ContextualAction)?;
    ///     Ok(binding.first.map_or_else(|| "none".to_owned(), |key| key.to_string()))
    /// }
    /// ```
    #[script]
    pub fn binding(self, action: &GameplayAction) -> Result<Binding, BindingError> {
        let host = self.0.host;
        let action = name(action)?;
        let mut out = GameBinding::EMPTY;
        let status = (host.api().game_binding)(host.handle(), Str::new(&action), &raw mut out);
        answered(status, action, None)?;
        Ok(Binding {
            first: BoundKey::from_bits(out.first),
            second: BoundKey::from_bits(out.second),
        })
    }

    /// Binds `key` as the first key of the game's control `action`, in the
    /// player's keyboard and mouse bindings, until [`Input::reset_binding`]
    /// gives it back or the mod turns off; the second key stays. The game's
    /// input and its key glyphs follow at the interface's next frame, and
    /// the loader writes the key again each time the game rebuilds its
    /// bindings from a change the player applies, so one call holds. The
    /// game's Controls page still shows the player's own key, and the game
    /// saves only that. Of the mods that rebind one action, the one that
    /// rebound it last wins, and asking again for the same rebind keeps a
    /// mod's place. Client only, on any thread.
    ///
    /// # Errors
    ///
    /// As [`Input::binding`], and [`BindingError::Refused`] for a key whose
    /// modifier the game's bindings do not list.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::controls::Key;
    /// use enshwick::input::BindingError;
    /// use enshwick::typed::input_mapping::runtime::GameplayAction;
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let input = load.input()?;
    ///     let jump = Key::parse("J")?;
    ///     let mut asked = false;
    ///     load.on_frame(move |_| {
    ///         if !asked {
    ///             // Unavailable until the interface's first frame read them.
    ///             let rebound = input.rebind(&GameplayAction::Jump, jump);
    ///             asked = rebound != Err(BindingError::Unavailable);
    ///         }
    ///     })
    /// }
    /// ```
    #[script]
    pub fn rebind(self, action: &GameplayAction, key: Key) -> Result<(), BindingError> {
        write(self.0.host, action, Some(key))
    }

    /// Ends this mod's rebind of the game's control `action`, which then
    /// takes another mod's rebind of it or else the first key the player's
    /// own bindings hold; nothing for an action this mod does not rebind.
    /// Client only, on any thread.
    ///
    /// # Errors
    ///
    /// As [`Input::binding`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::typed::input_mapping::runtime::GameplayAction;
    ///
    /// fn give_back(input: enshwick::input::Input) -> Result<(), enshwick::input::BindingError> {
    ///     input.reset_binding(&GameplayAction::Jump)
    /// }
    /// ```
    #[script]
    pub fn reset_binding(self, action: &GameplayAction) -> Result<(), BindingError> {
        write(self.0.host, action, None)
    }
}

#[cfg(test)]
mod tests {
    use crate::script::FromScript as _;

    use super::*;

    #[test]
    fn the_key_form_reads_as_a_key_a_reserved_key_or_other_input() {
        let e = Key::parse("Shift+E").unwrap();
        assert_eq!(BoundKey::from_bits(e.bits()), Some(BoundKey::Key(e)));
        assert_eq!(
            BoundKey::from_bits(0x2a),
            Some(BoundKey::Reserved("LeftShift"))
        );
        assert_eq!(
            BoundKey::from_bits(GameBinding::OTHER),
            Some(BoundKey::Other)
        );
        assert_eq!(BoundKey::from_bits(0x5f), Some(BoundKey::Other), "no name");
        assert_eq!(BoundKey::from_bits(0), None);
    }

    #[test]
    fn a_bound_key_crosses_into_a_script_and_back_as_its_name() {
        use crate::script::IntoScript as _;
        for key in [
            BoundKey::Key(Key::parse("Ctrl+Mouse4").unwrap()),
            BoundKey::Reserved("Escape"),
            BoundKey::Other,
        ] {
            assert_eq!(BoundKey::from_value(key.into_value()).unwrap(), key);
        }
        assert!(BoundKey::from_value(crate::script::Value::String("Hyper".into())).is_err());
    }
}
