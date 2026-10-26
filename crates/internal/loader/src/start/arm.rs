//! Arming the hook chains, mid-function hooks and code patches.

use crate::host::{self, Loader};
use crate::locks::Relock;
use crate::patch::{self, FrameSlot};

/// Arms every chain and mid-function hook past the mods that are off; an
/// address that fails turns off the mods hooked on it.
pub(super) fn arm(loader: &Loader, frame: Option<FrameSlot>) {
    let mut chains = loader.chains.relock();
    let mut mids = loader.mids.relock();
    chains.seal();
    mids.seal();
    let patches = crate::code::seal();
    let patch::Armed {
        failures,
        code: verdicts,
    } = patch::arm(
        &mut chains,
        &mids,
        frame,
        host::is_off,
        &patches,
        patch::Count::from_settings(),
    );
    let armed = (chains.len() + mids.len()).saturating_sub(failures.len());
    let owners: Vec<(usize, u64)> = failures
        .iter()
        .flat_map(|f| {
            let owners = if f.inside {
                mids.owners(f.target)
            } else {
                chains.owners(f.target)
            };
            owners.into_iter().map(move |owner| (owner, f.target))
        })
        .collect();
    drop(mids);
    drop(chains);
    crate::inventory::settle(
        loader,
        failures.iter().filter(|f| f.inside).map(|f| f.target),
    );
    for failure in &failures {
        crate::log!(
            ERROR,
            "hook at {:#x} not armed: {}",
            failure.target,
            failure.reason
        );
    }
    for (owner, target) in owners {
        loader.turn_off(
            owner,
            &format!("its hook at {target:#x} could not be armed"),
        );
    }
    crate::log!(INFO, "{armed} hook targets armed");
    if crate::diag::timing() {
        crate::log!(INFO, "diagnostics on: hook calls counted, one in 256 timed");
    } else if crate::diag::counting() {
        crate::log!(INFO, "diagnostics on: hook calls counted");
    }
    report_patches(loader, &patches, &verdicts);
}

/// Logs what became of each code patch and turns off the owners of skipped
/// ones.
fn report_patches(
    loader: &Loader,
    patches: &[crate::code::CodePatch],
    verdicts: &[crate::code::Verdict],
) {
    let mut tally = crate::diag::Patches::default();
    for verdict in verdicts {
        match verdict {
            crate::code::Verdict::Write => tally.written += 1,
            crate::code::Verdict::Off => tally.off += 1,
            crate::code::Verdict::Skip(_) => tally.skipped += 1,
        }
    }
    crate::diag::record_patches(tally);
    for (patch, verdict) in patches.iter().zip(verdicts) {
        let owner = loader
            .mods
            .get(patch.owner)
            .map_or(crate::settings::LOADER_ID, |m| m.id.as_str());
        let len = patch.replacement.len();
        match verdict {
            crate::code::Verdict::Write => crate::log!(
                INFO,
                "code patch of {owner} at {:#x} written, {len} bytes",
                patch.address
            ),
            crate::code::Verdict::Off => crate::log!(
                INFO,
                "code patch of {owner} at {:#x} left out: the mod is off",
                patch.address
            ),
            crate::code::Verdict::Skip(reason) => {
                crate::log!(
                    ERROR,
                    "code patch of {owner} at {:#x} skipped: {reason}",
                    patch.address
                );
                loader.turn_off(
                    patch.owner,
                    &format!("its code patch at {:#x} was skipped", patch.address),
                );
            }
        }
    }
    if crate::code::lifted() {
        crate::log!(INFO, "workshop recipe limit lifted");
    }
}
