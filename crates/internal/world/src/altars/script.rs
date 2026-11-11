//! Flame Altars as a script sees them: the namespace `altars`, its handles
//! as objects, and the boxes a building zone check hands a filter.

#[cfg(feature = "unstable")]
use super::set_base_cap;
use super::{
    Altar, Altars, Base, BaseLimit, Bounds, BuildingZoneCheck, Grouping, ZoneSizes, base_limit,
    bases_of, cell_of_fixed, cell_of_metres, grouping,
};
use crate::Off;
use crate::script::{Class, Field, Function, IntoScript, Object, Record, Ty, Typed, Value};

impl Object for Altars {
    const CLASS: &'static Class = &Class::of::<Self>(
        "Altars",
        "The world's Flame Altars and the bases they make.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_LIST];
}

impl Object for ZoneSizes {
    const CLASS: &'static Class = &Class::of::<Self>(
        "ZoneSizes",
        "The altars' zone sizes a filter changed, to tell the game when they change again.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_SIZES];
}

impl Typed for BuildingZoneCheck<'_> {
    const TY: &'static Ty = &Ty::Record(&Record::new(
        "BuildingZoneCheck",
        "An object a player places, checked against the building zones it may reach into.",
        &[
            Field::new(
                "zones",
                "The building zones, half-open.",
                &Ty::List(<Bounds as Typed>::TY),
            ),
            Field::new(
                "object",
                "The object's box, both corners included.",
                <Bounds as Typed>::TY,
            ),
            Field::new(
                "margin",
                "How far an object reaches past a zone.",
                &Ty::Number,
            ),
        ],
    ));
}

impl IntoScript for BuildingZoneCheck<'_> {
    #[expect(
        clippy::cast_precision_loss,
        reason = "a margin in cells is far below 2^53"
    )]
    fn into_value(self) -> Value {
        Value::Record(vec![
            Value::List(self.zones.iter().map(|z| z.into_value()).collect()),
            self.object.into_value(),
            Value::Number(self.margin as f64),
        ])
    }
}

#[crate::script(helpers)]
#[expect(
    clippy::unused_self,
    reason = "a script call takes its namespace, though a free function reads nothing of it"
)]
impl Altars {
    /// The cell of a position in the game's 32.32 fixed point.
    #[script(name = "cell_of_fixed")]
    pub(crate) fn cell_of_fixed_script(self, fixed: [i64; 3]) -> [i64; 3] {
        cell_of_fixed(fixed)
    }

    /// The cell of a position in metres.
    #[script(name = "cell_of_metres")]
    pub(crate) fn cell_of_metres_script(self, metres: [f64; 3]) -> [i64; 3] {
        cell_of_metres(metres)
    }

    /// The bases `altars` make, as `bases_of` groups them.
    #[script(name = "bases_of")]
    pub(crate) fn bases_of_script(self, altars: &[Altar]) -> Vec<Base> {
        bases_of(altars)
    }

    /// The limit of bases in force, as `base_limit` reads it.
    #[script(name = "base_limit")]
    pub(crate) fn base_limit_script(self) -> Option<BaseLimit> {
        base_limit()
    }

    /// Sets the world's cap of bases, as `set_base_cap` does.
    ///
    /// # Errors
    ///
    /// As [`set_base_cap`].
    #[cfg(feature = "unstable")]
    #[script(name = "set_base_cap")]
    pub(crate) fn set_base_cap_script(self, bases: u32) -> Result<(), Off> {
        set_base_cap(bases)
    }

    /// How the game groups altars into bases, as `grouping` reads it.
    #[script(name = "grouping")]
    pub(crate) fn grouping_script(self) -> Option<Grouping> {
        grouping()
    }
}
