//! Holds on the game: a mod that keeps a world the vanilla game cannot open
//! safely sets one (`enshwick::state::Hold`), and the loader stops the
//! game before the world loads when that mod would not run.

/// How long the log gets to write the reasons before the game stops.
#[cfg(windows)]
const LOG_GRACE: std::time::Duration = std::time::Duration::from_secs(1);
/// The exit code of a game stopped by a hold.
#[cfg(windows)]
const EXIT_HELD: u32 = 3;

/// The holds that stop the game: every one when the build is unknown, else
/// those of mods that do not run.
#[must_use]
pub fn unmet(
    holds: Vec<(String, String)>,
    running: &[&str],
    unknown_build: bool,
) -> Vec<(String, String)> {
    holds
        .into_iter()
        .filter(|(id, _)| unknown_build || !running.contains(&id.as_str()))
        .collect()
}

/// Stops the game when a hold under `data_dir` is unmet; see [`unmet`].
#[cfg(windows)]
pub(crate) fn enforce(data_dir: &std::path::Path, running: &[&str], unknown_build: bool) {
    let held = unmet(enshwick::state::holds(data_dir), running, unknown_build);
    if held.is_empty() {
        return;
    }
    let why = if unknown_build {
        "the game build is unknown, so it would run vanilla"
    } else {
        "the mod does not run"
    };
    for (id, reason) in &held {
        crate::log!(
            ERROR,
            "{id} holds this game ({reason}) and {why}: the game stops before the world loads; \
             update Enshwick and the mod, or delete mod-data/{id}/{} to open the world anyway, \
             at your own risk",
            enshwick::state::HOLD_FILE
        );
    }
    std::thread::sleep(LOG_GRACE);
    // SAFETY: no preconditions; the pseudo handle of this process.
    let me = unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() };
    // SAFETY: a valid process handle; nothing of the game's world is open yet.
    unsafe { windows_sys::Win32::System::Threading::TerminateProcess(me, EXIT_HELD) };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holds() -> Vec<(String, String)> {
        vec![
            ("you.lanterns".to_owned(), "20 zones".to_owned()),
            ("other".to_owned(), "why".to_owned()),
        ]
    }

    #[test]
    fn a_running_mod_meets_its_hold() {
        let left = unmet(holds(), &["you.lanterns"], false);
        assert_eq!(left, vec![("other".to_owned(), "why".to_owned())]);
        assert!(unmet(holds(), &["you.lanterns", "other"], false).is_empty());
    }

    #[test]
    fn an_unknown_build_meets_no_hold() {
        assert_eq!(unmet(holds(), &["you.lanterns", "other"], true).len(), 2);
        assert!(unmet(Vec::new(), &[], true).is_empty());
    }
}
