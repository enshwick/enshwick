//! Arming the hook chains, mid-function hooks and code patches.

use crate::engine::fuse::{Fuse, Off};
use crate::engine::loader::Loader;
use crate::engine::mod_index::ModIndex;
use crate::engine::patch::{self, FrameSlot};
use enshwick_base::Relock;

/// Arms every chain and mid-function hook past the mods that are off; an
/// address that fails turns off the mods hooked on it.
pub(super) fn arm(loader: &Loader, frame: Option<FrameSlot>) {
    let mut chains = loader.chains.relock();
    let mut mids = loader.mids.relock();
    // Drops the links of features that turned off while linking, so a
    // feature arms all its links or none.
    chains.seal();
    mids.seal();
    let patches = crate::engine::code::seal();
    let patch::Armed {
        failures,
        code: verdicts,
    } = patch::arm(
        &mut chains,
        &mids,
        frame,
        crate::engine::loader::bypassed,
        crate::engine::loader::wait,
        &patches,
        patch::Count::from_settings(),
    );
    let armed = (chains.len() + mids.len()).saturating_sub(failures.len());
    crate::engine::shipped::release();
    let owners: Vec<(ModIndex, u64, &str)> = failures
        .iter()
        .flat_map(|f| {
            let owners = if f.inside {
                mids.owners(f.target)
            } else {
                chains.owners(f.target)
            };
            // The loader's chain links go off with their features below.
            owners
                .into_iter()
                .filter(move |owner| f.inside || !owner.is_loader())
                .map(move |owner| (owner, f.target, f.reason.as_str()))
        })
        .collect();
    let features: Vec<(&'static Fuse, u64, &str)> = failures
        .iter()
        .filter(|f| !f.inside)
        .flat_map(|f| {
            chains
                .features(f.target)
                .map(move |feature| (feature, f.target, f.reason.as_str()))
        })
        .collect();
    let hooked: Vec<u64> = chains
        .iter()
        .filter(|c| c.head.is_some())
        .map(|c| c.target)
        .collect();
    drop(mids);
    drop(chains);
    crate::inventory::transactions::settle(
        loader,
        failures.iter().filter(|f| f.inside).map(|f| f.target),
    );
    let named = |target: u64| {
        crate::engine::diag::name_of(target).unwrap_or_else(|| format!("{target:#x}"))
    };
    for failure in &failures {
        let id = named(failure.target);
        let reason = format!("could not be armed: {}", failure.reason);
        crate::log!(
            ERROR,
            "{}",
            crate::engine::builds::off_line("hook", &loader.build.name, &id, &reason)
        );
    }
    for (feature, target, reason) in features {
        let reason = format!("could not be armed: {reason}");
        let off = Off::build(
            feature.feature(),
            &loader.build.name,
            &named(target),
            &reason,
        );
        feature.turn_off(&off);
    }
    for (owner, target, reason) in owners {
        let id = named(target);
        if owner.is_loader() {
            // A mid-function hook of the loader names no feature yet; the
            // hooked function stands for it.
            let reason = format!("could not be armed: {reason}");
            let off = Off::build(&id, &loader.build.name, "its hook", &reason);
            Fuse::named(&id).turn_off(&off);
        } else {
            loader.turn_off(
                owner,
                &format!("its hook on {id} could not be armed: {reason}"),
            );
        }
    }
    crate::log!(INFO, "{armed} hook targets armed");
    if crate::engine::diag::timing() {
        crate::log!(INFO, "diagnostics on: hook calls counted, one in 256 timed");
    } else if crate::engine::diag::counting() {
        crate::log!(INFO, "diagnostics on: hook calls counted");
    }
    let reported = report_patches(loader, &patches, &verdicts);
    crate::world::bases::report(loader, &reported);
    crate::engine::retry::hold(loader, patches, verdicts, hooked);
}

/// Logs what became of each code patch and turns off the owners of skipped
/// ones: a mod whole, a loader feature by its name. The loader features
/// turned off or waiting, each once.
fn report_patches(
    loader: &Loader,
    patches: &[crate::engine::code::CodePatch],
    verdicts: &[crate::engine::code::Verdict],
) -> Vec<&'static str> {
    use crate::engine::code::Verdict;

    let mut reported: Vec<&'static str> = Vec::new();
    crate::engine::diag::record_patches(crate::engine::code::tally(verdicts));
    for (patch, verdict) in patches.iter().zip(verdicts) {
        let owner = crate::engine::code::owner_id(loader, patch.owner);
        let len = patch.replacement.len();
        match verdict {
            Verdict::Write => crate::log!(
                INFO,
                "code patch of {owner} at {:#x} written, {len} bytes",
                patch.address
            ),
            Verdict::Off => crate::log!(
                INFO,
                "code patch of {owner} at {:#x} left out: the mod is off",
                patch.address
            ),
            Verdict::Wait => {
                crate::log!(
                    WARN,
                    "code patch of {owner} at {:#x} waits: a thread stayed inside it; \
                     tried again after the next game step, then up to three times a minute apart, \
                     its hooks off until then",
                    patch.address
                );
                if patch.owner.is_loader() && !reported.contains(&patch.feature) {
                    reported.push(patch.feature);
                }
            }
            Verdict::Skip(reason) => {
                crate::engine::code::skipped(loader, patch, reason, &mut reported);
            }
        }
    }
    if crate::engine::code::lifted() {
        crate::log!(INFO, "workshop recipe limit lifted");
    }
    reported
}
