//! Typed services and mod events through the fake loader.

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off};

    /// What `trading-post` publishes today.
    #[enshwick::service(name = "trading-post:prices")]
    pub(crate) struct Prices {
        pub(crate) price: extern "system" fn(item: u32) -> u32,
        pub(crate) refund: extern "system" fn(item: u32) -> u32,
    }

    /// What an older `trading-post` published.
    mod older {
        #[enshwick::service(name = "trading-post:prices")]
        pub(crate) struct Prices {
            pub(crate) price: extern "system" fn(item: u32) -> u32,
        }
    }

    /// The same table with a changed field.
    mod changed {
        #[enshwick::service(name = "trading-post:prices")]
        pub(crate) struct Prices {
            pub(crate) price: extern "system" fn(item: u64) -> u32,
            pub(crate) refund: extern "system" fn(item: u32) -> u32,
        }
    }

    #[enshwick::event(name = "trading-post:sold")]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Sold {
        pub(crate) item: u32,
        pub(crate) price: u32,
    }

    /// A newer `trading-post:sold`, with a field this mod does not know.
    mod newer {
        #[enshwick::event(name = "trading-post:sold")]
        #[derive(Clone, Copy, Debug)]
        pub(crate) struct Sold {
            pub(crate) item: u32,
            pub(crate) price: u32,
            pub(crate) buyer: u64,
        }
    }

    #[enshwick::service(name = "market:stalls", version = 2)]
    pub(crate) struct Stalls {
        pub(crate) count: extern "system" fn() -> u32,
    }

    #[enshwick::event(name = "market:opened")]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Opened {
        pub(crate) stalls: u32,
    }

    extern "system" fn price(item: u32) -> u32 {
        item * 10
    }

    extern "system" fn refund(item: u32) -> u32 {
        item
    }

    extern "system" fn wide_price(item: u64) -> u32 {
        u32::try_from(item).unwrap_or(0)
    }

    extern "system" fn count() -> u32 {
        4
    }

    static PRICES: Prices = Prices { price, refund };
    static OLDER: older::Prices = older::Prices { price };
    static CHANGED: changed::Prices = changed::Prices {
        price: wide_price,
        refund,
    };
    static STALLS: Stalls = Stalls { count };
    static EARNED: AtomicU32 = AtomicU32::new(0);

    struct Market;

    impl Mod for Market {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "market", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let prices: &'static Prices = load.service()?;
            assert_eq!((prices.price)(3), 30);
            load.publish(&STALLS)?;
            let opened = load.define_event::<Opened>()?;
            opened.send(&Opened { stalls: 4 });
            load.on_mod_event(|sold: &Sold| {
                EARNED.fetch_add(sold.price, Ordering::Relaxed);
            })?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Market);

    fn start(loader: &enshwick::test::Installed) -> Status {
        // SAFETY: both functions are the exports of this file's `declare_mod!(Market)`.
        unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) }
    }

    fn logged(loader: &enshwick::test::Installed) -> String {
        loader
            .logs()
            .into_iter()
            .map(|(_, line)| line)
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_mod_uses_publishes_defines_and_subscribes() {
        let loader = enshwick::test::FakeHost::new().service(&PRICES).install();
        assert_eq!(start(&loader), Status::OK);
        assert_eq!(loader.published(), [("market:stalls".to_owned(), 2)]);
        assert_eq!(loader.defined_events(), ["market:opened"]);
        assert_eq!(loader.sent_events(), ["market:opened"]);
        EARNED.store(0, Ordering::Relaxed);
        assert_eq!(loader.send_mod_event(&Sold { item: 1, price: 7 }), 1);
        assert_eq!(EARNED.load(Ordering::Relaxed), 7);
    }

    #[test]
    fn a_newer_event_reaches_an_older_subscriber() {
        let loader = enshwick::test::FakeHost::new().service(&PRICES).install();
        assert_eq!(start(&loader), Status::OK);
        EARNED.store(0, Ordering::Relaxed);
        let sold = newer::Sold {
            item: 1,
            price: 5,
            buyer: 9,
        };
        assert_eq!(loader.send_mod_event(&sold), 1);
        assert_eq!(EARNED.load(Ordering::Relaxed), 5);
    }

    #[test]
    fn an_older_publisher_is_refused_with_the_missing_field() {
        let loader = enshwick::test::FakeHost::new().service(&OLDER).install();
        assert_ne!(start(&loader), Status::OK);
        let log = logged(&loader);
        assert!(log.contains("trading-post:prices v1"), "{log}");
        assert!(log.contains("`refund`"), "{log}");
    }

    #[test]
    fn a_changed_field_is_refused() {
        let loader = enshwick::test::FakeHost::new().service(&CHANGED).install();
        assert_ne!(start(&loader), Status::OK);
        let log = logged(&loader);
        assert!(log.contains("field 0 is `price`"), "{log}");
    }

    #[test]
    fn a_missing_publisher_is_named() {
        let loader = enshwick::test::FakeHost::new().install();
        assert_ne!(start(&loader), Status::OK);
        let log = logged(&loader);
        assert!(log.contains("no running mod publishes it"), "{log}");
    }
}
