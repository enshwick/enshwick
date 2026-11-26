//! The lines of queued chat commands, kept by command id from the queue
//! until the interface frame posts them or the command ends.

use std::sync::Mutex;

use enshwick_base::Relock;

/// The chat commands' state, one per process.
struct State {
    /// The parked lines, by command id; one per mod at most, as one command
    /// of each kind waits per mod.
    parked: Mutex<Vec<(u64, String)>>,
}

static STATE: State = State {
    parked: Mutex::new(Vec::new()),
};

/// Keeps command `id`'s line until it is posted or ends.
pub(super) fn park(id: u64, line: String) {
    STATE.parked.relock().push((id, line));
}

/// What `post` answers for command `id`'s line, lent while it is parked, so
/// a frame that retries a full queue copies nothing; `None` without one.
pub(super) fn with_line<T>(id: u64, post: impl FnOnce(&str) -> T) -> Option<T> {
    STATE
        .parked
        .relock()
        .iter()
        .find(|(parked, _)| *parked == id)
        .map(|(_, line)| post(line))
}

/// Forgets command `id`'s line.
pub(super) fn release(id: u64) {
    STATE.parked.relock().retain(|(parked, _)| *parked != id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parked_line_stays_until_its_command_releases_it() {
        park(91, "hello".to_owned());
        park(92, "there".to_owned());
        assert_eq!(with_line(91, str::to_owned).as_deref(), Some("hello"));
        release(91);
        assert_eq!(with_line(91, str::to_owned), None);
        assert_eq!(
            with_line(92, str::to_owned).as_deref(),
            Some("there"),
            "another's stays"
        );
        release(92);
    }
}
