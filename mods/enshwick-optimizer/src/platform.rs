//! Where the mod runs and which of its changes are on there by default.
//!
//! [`DEFAULTS`] is the one table of what is on where. A cell is on only when
//! A/B runs on test servers showed the change saves CPU there and changes
//! nothing players notice; the README lists the numbers. Everywhere else the
//! change stays off unless its setting forces it.

use core::fmt;

/// What the game is and what it runs on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Platform {
    /// The dedicated server on Windows.
    WindowsServer,
    /// The dedicated server under Wine or Proton.
    WineServer,
    /// The game on Windows, hosting a game or not.
    WindowsClient,
    /// The game under Wine or Proton, hosting a game or not.
    WineClient,
}

impl Platform {
    /// Every platform, in the order of the columns of [`DEFAULTS`].
    #[cfg(test)]
    pub(crate) const ALL: [Self; 4] = [
        Self::WindowsServer,
        Self::WineServer,
        Self::WindowsClient,
        Self::WineClient,
    ];

    /// The platform of a server or client under Wine or not.
    pub(crate) const fn of(server: bool, wine: bool) -> Self {
        match (server, wine) {
            (true, false) => Self::WindowsServer,
            (true, true) => Self::WineServer,
            (false, false) => Self::WindowsClient,
            (false, true) => Self::WineClient,
        }
    }

    const fn column(self) -> usize {
        match self {
            Self::WindowsServer => 0,
            Self::WineServer => 1,
            Self::WindowsClient => 2,
            Self::WineClient => 3,
        }
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::WindowsServer => "Windows server",
            Self::WineServer => "Wine server",
            Self::WindowsClient => "Windows client",
            Self::WineClient => "Wine client",
        })
    }
}

/// One of the mod's changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Change {
    /// Sleeping the last fraction of a millisecond of the event loop's waits.
    ExactWaits,
    /// Capping the task workers to the container's CPU limit.
    TaskWorkers,
    /// Lengthening the server loop's wait while players are online.
    ServerWaitCap,
}

impl fmt::Display for Change {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ExactWaits => "exact waits",
            Self::TaskWorkers => "task workers capped to the CPU limit",
            Self::ServerWaitCap => "server wait cap",
        })
    }
}

/// Whether each change is on by default, per platform: Windows server, Wine
/// server, Windows client, Wine client.
pub(crate) const DEFAULTS: [(Change, [bool; 4]); 3] = [
    (Change::ExactWaits, [true, true, false, false]),
    (Change::TaskWorkers, [false, true, false, false]),
    (Change::ServerWaitCap, [false, false, false, false]),
];

/// Whether `change` is on by default on `platform`.
pub(crate) fn default_on(change: Change, platform: Platform) -> bool {
    DEFAULTS
        .iter()
        .find(|(c, _)| *c == change)
        .is_some_and(|(_, on)| on[platform.column()])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_covers_every_change_once() {
        for change in [
            Change::ExactWaits,
            Change::TaskWorkers,
            Change::ServerWaitCap,
        ] {
            assert_eq!(DEFAULTS.iter().filter(|(c, _)| *c == change).count(), 1);
        }
    }

    #[test]
    fn the_defaults_follow_the_measurements() {
        assert!(default_on(Change::ExactWaits, Platform::WindowsServer));
        assert!(default_on(Change::ExactWaits, Platform::WineServer));
        assert!(!default_on(Change::ExactWaits, Platform::WindowsClient));
        assert!(default_on(Change::TaskWorkers, Platform::WineServer));
        assert!(!default_on(Change::TaskWorkers, Platform::WindowsServer));
        for platform in Platform::ALL {
            assert!(!default_on(Change::ServerWaitCap, platform));
        }
        assert_eq!(Platform::of(true, true), Platform::WineServer);
        assert_eq!(Platform::of(false, false).to_string(), "Windows client");
    }
}
