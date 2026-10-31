//! Mod events described at run time reach a handler as the typed event's
//! bytes, a described definition sends its bytes, and an event's schema
//! goes into its definer's manifest.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::points::DescribedField;
    use enshwick::services::{DescribedEvent, ModEvent};
    use enshwick::{Load, Mod, Off};

    /// A sale at the trading post.
    #[enshwick::event(name = "you.trading-post:sold")]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Sold {
        /// The item.
        pub(crate) item: u32,
        /// How many.
        pub(crate) count: u32,
        /// The buyer's account.
        pub(crate) player: u64,
    }

    static SEEN: Mutex<Vec<Vec<u8>>> = Mutex::new(Vec::new());
    static SIZE: Mutex<u32> = Mutex::new(16);

    fn described(size: u32) -> DescribedEvent {
        let fields = Sold::LAYOUT
            .iter()
            .map(|f| {
                // SAFETY: the event macro's layout names are static strings.
                DescribedField::new(unsafe { f.name.as_str() }.unwrap_or(""), f.hash)
            })
            .collect();
        DescribedEvent::new(Sold::NAME, fields, size)
    }

    struct Watcher;

    impl Mod for Watcher {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let size = *SIZE.lock().unwrap();
            load.on_described_mod_event(&described(size), |data| {
                SEEN.lock().unwrap().push(data.to_vec());
            })?;
            // SAFETY: the fields are two `u32` and a `u64`, for which any
            // bytes are valid.
            let send = unsafe { load.define_described_event(&described(16)) }?;
            let mut data = [0u8; 16];
            data[..4].copy_from_slice(&3u32.to_le_bytes());
            send.send(&data);
            send.send(&data[..4]);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Watcher,
        r#"
        id = "you.trading-post"
        side = "server"
        "#,
        events: [Sold]
    );

    fn start(size: u32) -> enshwick::test::Installed {
        // After the install, which tests take in turn: set earlier, another
        // test's size or events could take this one's place.
        let loader = enshwick::test::FakeHost::new().install();
        *SIZE.lock().unwrap() = size;
        SEEN.lock().unwrap().clear();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Watcher)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    #[test]
    fn a_described_handler_gets_the_typed_event_s_bytes() {
        let loader = start(16);
        assert_eq!(
            loader.send_mod_event(&Sold {
                item: 7,
                count: 1,
                player: 9
            }),
            1
        );
        let seen = SEEN.lock().unwrap().clone();
        assert_eq!(seen.len(), 1);
        assert_eq!(&seen[0][..4], &7u32.to_le_bytes());
        assert_eq!(&seen[0][8..16], &9u64.to_le_bytes());
    }

    #[test]
    fn a_described_definition_sends_its_bytes_and_nothing_of_another_size() {
        let loader = start(16);
        assert_eq!(loader.sent_events(), [Sold::NAME]);
        assert_eq!(&loader.sent_event_data()[0][..4], &3u32.to_le_bytes());
    }

    #[test]
    fn a_handler_reading_past_the_event_gets_nothing() {
        let loader = start(64);
        assert_eq!(
            loader.send_mod_event(&Sold {
                item: 7,
                count: 1,
                player: 9
            }),
            0
        );
    }

    #[test]
    fn the_manifest_carries_the_event_s_schema() {
        // SAFETY: `enshwick_mod_query` returns the `'static` manifest of
        // `declare_mod!`, which no thread writes.
        let manifest = unsafe { &*enshwick_mod_query() };
        // SAFETY: `declare_mod!` points the schema at static text.
        let text = unsafe { manifest.points.as_str() }.unwrap_or("");
        assert!(
            text.contains("[[event]]\nname = \"you.trading-post:sold\"\ndata = \"Sold\""),
            "{text}"
        );
        assert!(
            text.contains("name = \"player\"\ndoc = \"The buyer's account.\"\ntype = \"u64\""),
            "{text}"
        );
    }
}
