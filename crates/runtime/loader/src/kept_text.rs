//! Text the loader hands mods for the life of the process, within a budget.

/// Texts kept for the process, each once, at most `budget` bytes of them.
#[derive(Debug)]
pub(crate) struct KeptText {
    texts: Vec<&'static str>,
    bytes: usize,
    budget: usize,
}

impl KeptText {
    /// Nothing kept yet, with room for `budget` bytes.
    pub(crate) const fn new(budget: usize) -> Self {
        Self {
            texts: Vec::new(),
            bytes: 0,
            budget,
        }
    }

    /// `text` kept for the process: the copy kept before when there is one,
    /// a new copy while the budget has room, `None` once it has none.
    pub(crate) fn keep(&mut self, text: &str) -> Option<&'static str> {
        if let Some(&held) = self.texts.iter().find(|held| **held == text) {
            return Some(held);
        }
        let bytes = self
            .bytes
            .checked_add(text.len())
            .filter(|&bytes| bytes <= self.budget)?;
        let held: &'static str = Box::leak(text.into());
        self.texts.push(held);
        self.bytes = bytes;
        Some(held)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_text_is_kept_once() {
        let mut kept = KeptText::new(64);
        let first = kept.keep("you.zones").unwrap();
        let second = kept.keep("you.zones").unwrap();
        assert_eq!(first.as_ptr(), second.as_ptr());
        assert_eq!(kept.bytes, "you.zones".len());
    }

    #[test]
    fn new_text_past_the_budget_is_refused_and_kept_text_still_reads() {
        let mut kept = KeptText::new(8);
        assert_eq!(kept.keep("12345"), Some("12345"));
        assert_eq!(kept.keep("6789"), None);
        assert_eq!(kept.keep("678"), Some("678"));
        assert_eq!(kept.keep(""), Some(""));
        assert_eq!(kept.keep("9"), None);
        assert_eq!(kept.keep("12345"), Some("12345"));
        assert_eq!(kept.bytes, 8);
    }

    #[test]
    fn a_server_sending_new_text_on_every_join_cannot_grow_the_keep_past_its_budget() {
        let mut kept = KeptText::new(1024);
        let held = (0..10_000)
            .filter_map(|i| kept.keep(&format!("server.mod-{i}")))
            .count();
        assert!(kept.bytes <= 1024, "{}", kept.bytes);
        assert!(held < 100, "{held}");
    }
}
