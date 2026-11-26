//! Content a mod serves: kept for the process up to the loader's limit,
//! past which the fake host refuses it as the loader does.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::test::FakeHost;
    use enshwick::{Load, Mod, Off};

    /// The loader's limit on contents served in one process.
    const MAX_ENTRIES: usize = 3072;

    struct Pixels;

    impl Mod for Pixels {
        fn init(_load: &mut Load<'_>) -> Result<Self, Off> {
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Pixels,
        r#"
        id = "you.pixels"
        side = "both"
        "#
    );

    #[test]
    fn content_past_the_loader_s_limit_is_refused_and_content_served_before_still_answers() {
        let loader = FakeHost::new().install();
        // SAFETY: the three functions are the exports of this file's `declare_mod!(Pixels)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let host = enshwick::host().unwrap();
        let first = host.serve_content(&[1]).unwrap();
        for len in 2..=MAX_ENTRIES {
            host.serve_content(&vec![1; len]).unwrap();
        }
        assert_eq!(host.serve_content(&[1]).unwrap(), first, "served before");
        assert!(host.serve_content(&[2; 8000]).is_err(), "past the limit");
        assert_eq!(loader.served_content().len(), MAX_ENTRIES);
    }
}
