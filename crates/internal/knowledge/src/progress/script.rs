//! What only a script's filters get of progress: the level table read
//! whole, and the filters that take it.

use super::LoadMethods as _;
use super::{Comfort, Experience, Levels};
use crate::Off;
use crate::script::{Class, Function, Object};

/// The [`Levels`] a script's filter gets: the table read whole during the
/// pass, since the loader's token lasts only that long.
#[derive(Clone, Debug)]
pub struct ScriptLevels {
    cap: u32,
    /// What each level from 1 up needs.
    needs: Vec<u32>,
}

impl ScriptLevels {
    /// Levels read past this are not copied: a real cap is far below it.
    const MAX_READ: u32 = 1024;

    /// Reads `levels` whole, while its pass lasts.
    fn read(levels: &Levels) -> Self {
        Self {
            cap: levels.cap(),
            needs: (1..=levels.cap().min(Self::MAX_READ))
                .map(|level| levels.need(level))
                .collect(),
        }
    }
}

impl Object for ScriptLevels {
    const CLASS: &'static Class = &Class::of::<Self>(
        "Levels",
        "The game's level table, during an experience pass.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_LEVELS];
}

#[crate::script(levels)]
impl ScriptLevels {
    /// The highest level, as [`Levels::cap`] answers.
    #[script(name = "cap")]
    pub(crate) const fn cap_script(&self) -> u32 {
        self.cap
    }

    /// What `level` needs, 0 for level 0 or past the cap, as
    /// [`Levels::need`] answers.
    #[script(name = "need")]
    pub(crate) fn need_script(&self, level: u32) -> u32 {
        level
            .checked_sub(1)
            .and_then(|at| self.needs.get(usize::try_from(at).ok()?))
            .copied()
            .unwrap_or(0)
    }
}

/// `Load`'s script-only methods of this module on the base handle, which
/// core's forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::on_comfort_script`.
    #[doc(hidden)]
    fn on_comfort_script(
        &mut self,
        filter: impl Fn(Comfort) -> Option<i32> + Send + Sync + 'static,
    ) -> Result<(), Off>;

    /// As `Load::on_experience_script`.
    #[doc(hidden)]
    fn on_experience_script(
        &mut self,
        filter: impl Fn(Vec<Experience>, ScriptLevels) -> Option<Vec<Experience>>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn on_comfort_script(
        &mut self,
        filter: impl Fn(Comfort) -> Option<i32> + Send + Sync + 'static,
    ) -> Result<(), Off> {
        self.on_comfort(move |comfort| filter(*comfort).unwrap_or(comfort.value))
    }

    fn on_experience_script(
        &mut self,
        filter: impl Fn(Vec<Experience>, ScriptLevels) -> Option<Vec<Experience>>
        + Send
        + Sync
        + 'static,
    ) -> Result<(), Off> {
        self.on_experience(move |all, levels| {
            if let Some(changed) = filter(all.to_vec(), ScriptLevels::read(levels)) {
                for (slot, new) in all.iter_mut().zip(changed) {
                    *slot = new;
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ScriptLevels;
    use crate::progress::Levels;

    unsafe extern "system" fn hundred_per_level(_token: *mut core::ffi::c_void, level: u32) -> u32 {
        level * 100
    }

    #[test]
    fn a_script_reads_the_level_table_as_the_pass_answered_it() {
        let levels = Levels {
            cap: 3,
            need_for: Some(hundred_per_level),
            token: core::ptr::null_mut(),
        };
        let read = ScriptLevels::read(&levels);
        assert_eq!(read.cap_script(), 3);
        assert_eq!(read.need_script(0), 0, "level 0 needs nothing");
        assert_eq!(read.need_script(2), 200);
        assert_eq!(read.need_script(3), 300);
        assert_eq!(read.need_script(4), 0, "past the cap needs nothing");
    }
}
