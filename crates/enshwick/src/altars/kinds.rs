//! Flame Altar kinds from mods: a prop copied from a vanilla altar whose
//! altars take zones of their own.

use crate::content::items::NewItem;
use crate::content::props::{NewProp, PropId};
use crate::{Load, Off};

use super::template_of;

/// A Flame Altar kind's zone at one level, as an altar zone filter sets it:
/// see [`AltarZone`](super::AltarZone).
///
/// # Examples
///
/// See [`Load::add_flame_altar`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct KindZone {
    /// Cells added to the vanilla zone on every horizontal side, at least 0.
    pub grow: i32,
    /// The zone's height in cells.
    pub height: i32,
}

/// A new Flame Altar kind for [`Load::add_flame_altar`]: a prop copied from
/// a vanilla altar, with its own zone at each level.
///
/// # Examples
///
/// See [`Load::add_flame_altar`].
#[derive(Clone, Debug, crate::Script)]
pub struct NewFlameAltar {
    /// The altar's item, a copy of a vanilla altar's item; its other ids
    /// derive from it as a prop's do.
    pub item: NewItem,
    /// Its zone at levels 1, 2 and on; a level past the list takes the last
    /// one, and an empty list keeps the vanilla zones.
    #[script(default)]
    pub zones: Vec<KindZone>,
    /// With a cost, the altar is built by a copy of the vanilla altar's
    /// recipe whose ingredient counts are times this.
    pub recipe_cost: Option<u32>,
}

#[crate::script(flame_altars)]
impl Load<'_> {
    /// Adds a kind of Flame Altar, on the server and the client alike: a
    /// prop copied from the vanilla altar whose item is `item.like`, as
    /// [`Load::add_prop`] adds one, whose altars take the kind's zones
    /// through an altar zone filter of this mod, chained with other mods'
    /// filters in load order (see [`Load::on_altar_zone`]).
    /// [`Altar::template`](super::Altar::template) tells the kind's altars
    /// apart.
    ///
    /// # Errors
    ///
    /// As [`Load::add_prop`] and [`Load::on_altar_zone`].
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::altars::{KindZone, NewFlameAltar};
    /// use enshwick::content::{Guid, id};
    /// use enshwick::content::items::NewItem;
    ///
    /// fn init(load: &mut enshwick::Load<'_>, altar: Guid) -> Result<(), enshwick::Off> {
    ///     let wide = load.add_flame_altar(NewFlameAltar {
    ///         item: NewItem::new(id!("wide-altar"), altar),
    ///         zones: vec![KindZone { grow: 20, height: 80 }],
    ///         recipe_cost: Some(2),
    ///     })?;
    ///     enshwick::info!("wide altars are {}", wide.template.guid());
    ///     Ok(())
    /// }
    /// ```
    #[script]
    pub fn add_flame_altar(&mut self, new: NewFlameAltar) -> Result<PropId, Off> {
        let prop = self.add_prop(NewProp {
            recipe_cost: new.recipe_cost,
            ..NewProp::new(new.item)
        })?;
        if let Some(&last) = new.zones.last() {
            let (host, kind, zones) = (self.host(), prop.template.guid(), new.zones);
            self.on_altar_zone(move |zone| {
                if template_of(host, zone.altar) != Some(kind) {
                    return;
                }
                let level = usize::from(zone.level.max(1)) - 1;
                let sized = zones.get(level).copied().unwrap_or(last);
                zone.grow = sized.grow.max(0).into();
                zone.height = sized.height.into();
            })?;
        }
        Ok(prop)
    }
}
