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
    /// [`Levels::need`] answers; 0 too past level 1024, which the table is
    /// not read beyond.
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
            // An empty pass gives the script nothing to change: spare it the
            // copies and the level table's reads.
            if all.is_empty() {
                return;
            }
            if let Some(changed) = filter(all.to_vec(), ScriptLevels::read(levels)) {
                write_back(all, changed);
            }
        })
    }
}

/// Writes each row a script returned over the pass's row of the same
/// character, so a list it filtered or reordered changes only those
/// characters; a row of a character not in the pass is dropped.
fn write_back(all: &mut [Experience], changed: Vec<Experience>) {
    for new in changed {
        if let Some(slot) = all.iter_mut().find(|slot| slot.entity == new.entity) {
            *slot = new;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ScriptLevels, write_back};
    use crate::Entity;
    use crate::progress::{Experience, Levels};

    fn row(entity: u32, gained: u32) -> Experience {
        Experience {
            entity: Entity(entity),
            level: 1,
            progress: 0,
            need: 100,
            gained,
        }
    }

    #[test]
    fn a_script_s_rows_go_back_to_their_own_characters_whatever_their_order() {
        let mut all = [row(1, 5), row(2, 6), row(3, 7)];
        write_back(&mut all, vec![row(3, 70), row(9, 90), row(1, 50)]);
        assert_eq!(
            all.map(|r| (r.entity, r.gained)),
            [(Entity(1), 50), (Entity(2), 6), (Entity(3), 70)],
            "character 2 keeps its row and character 9, not in the pass, is dropped"
        );
    }

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
