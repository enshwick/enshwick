//! What the cursor's survivor lookup answers.
//!
//! The cursor check that refuses a bed "too far" from a survivor runs on both
//! sides. A client alone measures from the first copy it knows, often one at
//! another altar, so it is told no copy exists. A server tells it no copy only
//! while copies exist, and the vanilla answer otherwise. A client that hosts a
//! game runs both cursors through one function, whose call sites cannot tell
//! them apart: it answers as a server, which suits both worlds, since the
//! answer depends only on the survivor.

/// The answer at a cursor call site.
///
/// `hosting` is whether this process runs a game server, always true on the
/// dedicated server; `client_site` whether the caller is the call site the
/// client alone is quieted at; `copies` how many copies of the survivor the
/// server knows; `found` the vanilla answer.
#[must_use]
pub(crate) fn answer(hosting: bool, client_site: bool, copies: usize, found: u32) -> u32 {
    match (hosting, client_site) {
        (true, _) if copies <= 1 => found,
        (true, _) | (false, true) => 0,
        (false, false) => found,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_server_quiets_the_cursor_only_while_copies_exist() {
        assert_eq!(answer(true, false, 0, 7), 7);
        assert_eq!(answer(true, false, 1, 7), 7);
        assert_eq!(answer(true, false, 2, 7), 0);
        assert_eq!(answer(true, true, 3, 7), 0);
        assert_eq!(answer(true, true, 1, 7), 7);
    }

    #[test]
    fn a_client_alone_is_quieted_at_its_own_site_only() {
        assert_eq!(answer(false, true, 0, 7), 0);
        assert_eq!(answer(false, true, 5, 7), 0);
        assert_eq!(answer(false, false, 5, 7), 7);
    }
}
