//! What the cursor's distance check is told.
//!
//! Before it offers to assign a bed, the cursor measures how far the bed lies
//! from the survivor and refuses one "too far". It measures from the entity
//! the lookup answers, often a copy at another altar. A server, or a client
//! while it hosts, skips the measure while copies exist and keeps it
//! otherwise; a client alone, which cannot tell the copies apart, always
//! skips it, as the server does when copies exist.

use enshwick_loader_api::survivors::Answer;

/// The cursor's answer: `hosting` is whether this process runs a game server,
/// always true on the dedicated server; `copies` how many copies of the
/// survivor the server knows.
#[must_use]
pub(crate) const fn answer(hosting: bool, copies: usize) -> Answer {
    if hosting && copies <= 1 {
        Answer::Vanilla
    } else {
        Answer::Nobody
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_skips_the_measure_only_while_copies_exist() {
        assert_eq!(answer(true, 0), Answer::Vanilla);
        assert_eq!(answer(true, 1), Answer::Vanilla);
        assert_eq!(answer(true, 2), Answer::Nobody);
    }

    #[test]
    fn a_client_alone_always_skips_it() {
        assert_eq!(answer(false, 0), Answer::Nobody);
        assert_eq!(answer(false, 5), Answer::Nobody);
    }
}
