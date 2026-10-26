//! A mod reads the language the player reads the game in.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::Status;
    use enshwick::i18n::Language;
    use enshwick::{Load, Mod, Off};

    static SEEN: AtomicU32 = AtomicU32::new(u32::MAX);

    struct Reader;

    impl Mod for Reader {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.reader", side: CLIENT)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            SEEN.store(load.host().language().0, Ordering::Relaxed);
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Reader);

    #[test]
    fn the_mod_reads_the_players_language() {
        let loader = enshwick::test::FakeHost::new()
            .language(Language::UK_UA)
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Reader)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        assert_eq!(SEEN.load(Ordering::Relaxed), Language::UK_UA.0);
        assert_eq!(enshwick::i18n::language(), Language::UK_UA);
    }
}
