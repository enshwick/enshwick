//! The rules the loader and the fake host share for the builds of a served
//! resource: which patches a build runs, when the game's first build keeps
//! its base, and what a rebuild or a served read answers a mod.

use crate::abi::Status;

/// Why a served resource was not rebuilt or read, as the loader and the fake
/// answer a mod. Exhaustive, so both handle every outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum BuildResourceError {
    /// The mod serves no such key.
    #[error("the mod serves no such resource")]
    NotServed,
    /// The key's patches run on this thread, which would wait on itself.
    #[error("the resource's patches run on this thread")]
    Building,
    /// No base was kept to build the key again from.
    #[error("no base was kept for the resource")]
    NoBase,
    /// No patch built the key.
    #[error("no patch built the resource")]
    NotBuilt,
    /// The rebuild made another size than the game holds.
    #[error("the rebuild made another size than the game holds")]
    SizeChanged,
    /// Asked before the registrations sealed.
    #[error("asked before the resources sealed")]
    Unsealed,
    /// The game's archive has no such resource.
    #[error("the archive has no such resource")]
    NotInArchive,
}

impl BuildResourceError {
    /// The status a mod's call gets: a key the mod does not serve is a bad
    /// argument, a build on this thread says so, a missing base, build or
    /// archive entry is missing, and a changed size or a read before the
    /// seal is refused.
    #[must_use]
    pub const fn status(self) -> Status {
        match self {
            Self::NotServed => Status::BAD_ARGS,
            Self::Building => Status::BUILDING,
            Self::NoBase | Self::NotBuilt | Self::NotInArchive => Status::MISSING,
            Self::SizeChanged | Self::Unsealed => Status::REFUSED,
        }
    }
}

/// Whether the patch of `owner` runs in a build of its served key.
///
/// In a rebuild, whose `rebuild` names the mods that built the bytes the
/// game holds, a patch of one of them runs, on or off, since the game holds
/// what each added; in the game's first build and a served read, a patch
/// still on of a mod still on runs, as `on` answers.
pub fn patch_runs<M: PartialEq>(
    rebuild: Option<&[M]>,
    owner: &M,
    on: impl FnOnce() -> bool,
) -> bool {
    match rebuild {
        Some(by) => by.contains(owner),
        None => on(),
    }
}

/// The reasons the game's first build of a served key keeps a copy of its
/// base for a rebuild; any one is enough.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct KeepBase {
    /// A mod asked during init to rebuild the key.
    pub asked: bool,
    /// A patch read another served key, which a rebuild of that key builds
    /// again.
    pub read_served: bool,
    /// The build asked whether the client shows served UI images before
    /// they did.
    pub asked_icons: bool,
}

impl KeepBase {
    /// Whether the first build keeps its base.
    #[must_use]
    pub const fn keeps(self) -> bool {
        self.asked || self.read_served || self.asked_icons
    }
}

#[cfg(test)]
mod tests {
    use super::{BuildResourceError, KeepBase, patch_runs};
    use crate::abi::Status;

    #[test]
    fn a_rebuild_runs_the_patches_of_the_mods_that_built_the_bytes_on_or_off() {
        assert!(patch_runs(Some(&[1, 2][..]), &2, || false));
        assert!(!patch_runs(Some(&[1][..]), &2, || true));
    }

    #[test]
    fn a_first_build_or_served_read_runs_only_patches_still_on() {
        assert!(patch_runs::<u32>(None, &2, || true));
        assert!(!patch_runs::<u32>(None, &2, || false));
    }

    #[test]
    fn the_first_build_keeps_its_base_for_any_of_the_three_reasons() {
        let none = KeepBase::default();
        assert!(!none.keeps());
        for reason in [
            KeepBase {
                asked: true,
                ..none
            },
            KeepBase {
                read_served: true,
                ..none
            },
            KeepBase {
                asked_icons: true,
                ..none
            },
        ] {
            assert!(reason.keeps(), "{reason:?}");
        }
    }

    #[test]
    fn each_outcome_answers_its_status() {
        let answers = [
            (BuildResourceError::NotServed, Status::BAD_ARGS),
            (BuildResourceError::Building, Status::BUILDING),
            (BuildResourceError::NoBase, Status::MISSING),
            (BuildResourceError::NotBuilt, Status::MISSING),
            (BuildResourceError::SizeChanged, Status::REFUSED),
            (BuildResourceError::Unsealed, Status::REFUSED),
            (BuildResourceError::NotInArchive, Status::MISSING),
        ];
        for (error, status) in answers {
            assert_eq!(error.status(), status, "{error:?}");
        }
    }
}
