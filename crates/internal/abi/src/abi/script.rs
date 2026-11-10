//! The ABI's own types as a script sees them.

use super::{Bounds, Level, Side};
use crate::script::{Choice, Error, Field, FromScript, IntoScript, Record, Ty, Typed, Value};

/// A side by name, as `mods.side` answers it.
impl Typed for Side {
    const TY: &'static Ty = &Ty::Choice(&Choice::new(
        "Side",
        "Which binary of the game runs, or where a mod runs.",
        &[
            (
                "server",
                "The dedicated server; as a mod's side, where the world runs.",
            ),
            (
                "client",
                "The game client, as a binary; never a mod's side.",
            ),
            (
                "both",
                "As a mod's side, where the world runs and on every client in it.",
            ),
        ],
    ));
}

impl IntoScript for Side {
    fn into_value(self) -> Value {
        let name = match self {
            Self::CLIENT => "client",
            Self::BOTH => "both",
            _ => "server",
        };
        Value::Name(name)
    }
}

impl FromScript for Side {
    fn from_value(value: Value) -> Result<Self, Error> {
        match String::from_value(value)?.as_str() {
            "server" => Ok(Self::SERVER),
            "client" => Ok(Self::CLIENT),
            "both" => Ok(Self::BOTH),
            other => Err(Error::new(format!(
                "{other:?} is not one of \"server\", \"client\", \"both\""
            ))),
        }
    }
}

/// A log level by name, as settings and log lines spell it.
impl Typed for Level {
    const TY: &'static Ty = &Ty::Choice(&Choice::new(
        "Level",
        "How much a log keeps, from errors alone to development detail.",
        &[
            ("error", "Something failed and a feature is off."),
            ("warn", "Something unexpected that the mod handled."),
            ("info", "A milestone worth one line."),
            ("debug", "Detail for development, off in releases."),
        ],
    ));
}

impl IntoScript for Level {
    fn into_value(self) -> Value {
        Value::Name(self.name())
    }
}

impl Typed for Bounds {
    const TY: &'static Ty = &Ty::Record(&Record::new(
        "Bounds",
        "A box of cells, by its lowest and highest corner.",
        &[
            Field::new("min", "The lowest corner.", <[i64; 3] as Typed>::TY),
            Field::new("max", "The highest corner.", <[i64; 3] as Typed>::TY),
        ],
    ));
}

impl IntoScript for Bounds {
    fn into_value(self) -> Value {
        Value::Record(vec![self.min.into_value(), self.max.into_value()])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_side_reads_back_from_its_name() {
        for (side, name) in [
            (Side::SERVER, "server"),
            (Side::CLIENT, "client"),
            (Side::BOTH, "both"),
        ] {
            assert!(matches!(side.into_value(), Value::Name(text) if text == name));
            assert_eq!(Side::from_value(Value::Name(name)).ok(), Some(side));
            assert_eq!(
                Side::from_value(Value::String(name.into())).ok(),
                Some(side)
            );
        }
    }
}
