//! Calls: a mod serves one to others and calls another mod's, typed and
//! described, and the manifest carries the served call's schema.

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use enshwick::abi::Status;
    use enshwick::points::DescribedField;
    use enshwick::services::{Call, CallError, Caller, DescribedCall, DescribedCaller};
    use enshwick::{Load, Mod, Off};

    /// What the trading post pays for an item.
    #[enshwick::call(name = "you.trading-post:price-of", export, output = u32, version = 2)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct PriceOf {
        /// The item.
        pub(crate) item: u32,
        /// Whether the seller is a regular.
        pub(crate) regular: bool,
        /// The item's grade.
        pub(crate) grade: u8,
        /// How many the seller sells.
        pub(crate) count: u16,
    }

    /// The rent of a stall, which another mod serves.
    #[enshwick::call(name = "you.market-stalls:rent", output = u64)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Rent {
        pub(crate) stall: u16,
    }

    /// The same call at a major nobody serves.
    #[enshwick::call(name = "you.market-stalls:rent", output = u64, version = 3)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct RentLater {
        pub(crate) stall: u16,
    }

    /// What `init` asked for: the typed callers and the described one.
    type Callers = (Caller<Rent>, Caller<RentLater>, DescribedCaller);

    /// A rate the mod serves from a description, as a script engine would.
    #[enshwick::call(name = "you.trading-post:rate", output = u32)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Rate {
        pub(crate) item: u32,
    }

    fn described<C: Call>(input_size: u32, output_size: u32) -> DescribedCall {
        let fields = |layout: &[enshwick::abi::LayoutField]| {
            layout
                .iter()
                // SAFETY: the call macro's layout names are static strings.
                .map(|f| DescribedField::new(unsafe { f.name.as_str() }.unwrap_or(""), f.hash))
                .collect()
        };
        DescribedCall::new(
            C::NAME,
            C::MAJOR,
            (fields(C::LAYOUT), input_size),
            (fields(C::OUTPUT), output_size),
        )
    }

    static CALLERS: Mutex<Option<Callers>> = Mutex::new(None);

    struct Post;

    impl Mod for Post {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.serve_call(|input: &PriceOf| {
                assert!(input.item != 13, "an unlucky item");
                if input.regular {
                    input.item * 12
                } else {
                    input.item * 10
                }
            })?;
            // SAFETY: the answer is a `u32`, for which any bytes are valid.
            unsafe {
                load.serve_described_call(&described::<Rate>(4, 4), |input, output| {
                    let item = u32::from_le_bytes(input.try_into().unwrap_or_default());
                    assert!(item != 13, "an unlucky item");
                    output.copy_from_slice(&(item * 2).to_le_bytes());
                    item != 0
                })?;
            }
            let rent = load.caller::<Rent>()?;
            let later = load.caller::<RentLater>()?;
            // SAFETY: the input is a `u16`, for which any bytes are valid.
            let described = unsafe { load.described_caller(&described::<Rent>(2, 8)) }?;
            *CALLERS.lock().unwrap() = Some((rent, later, described));
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Post,
        r#"
        id = "you.trading-post"
        side = "server"
        "#
    );

    fn start() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Post)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn callers() -> Callers {
        CALLERS.lock().unwrap().unwrap()
    }

    #[test]
    fn another_mod_calls_what_the_mod_serves() {
        let loader = start();
        assert_eq!(
            loader.served_calls(),
            ["you.trading-post:price-of v2", "you.trading-post:rate v1"]
        );
        assert_eq!(
            loader.call(&PriceOf {
                item: 3,
                regular: false,
                grade: 0,
                count: 1,
            }),
            Ok(30)
        );
        assert_eq!(
            loader.call(&PriceOf {
                item: 3,
                regular: true,
                grade: 0,
                count: 1,
            }),
            Ok(36)
        );
    }

    #[test]
    fn a_panicking_handler_fails_its_call_and_turns_off() {
        let loader = start();
        assert_eq!(
            loader.call(&PriceOf {
                item: 13,
                regular: false,
                grade: 0,
                count: 1,
            }),
            Err(CallError::Failed)
        );
        assert_eq!(
            loader.call(&PriceOf {
                item: 1,
                regular: false,
                grade: 0,
                count: 1,
            }),
            Err(CallError::Off)
        );
    }

    #[test]
    fn a_described_server_answers_a_typed_caller() {
        let loader = start();
        assert_eq!(loader.call(&Rate { item: 4 }), Ok(8));
        assert_eq!(loader.call(&Rate { item: 0 }), Err(CallError::Failed));
        assert_eq!(loader.call(&Rate { item: 5 }), Ok(10));
        assert_eq!(loader.call(&Rate { item: 13 }), Err(CallError::Failed));
        assert_eq!(loader.call(&Rate { item: 5 }), Err(CallError::Off));
    }

    #[test]
    fn the_mod_calls_another_mod_and_a_major_nobody_serves_is_unmatched() {
        let loader = start();
        let (rent, later, _) = callers();
        assert_eq!(rent.call(&Rent { stall: 4 }), Err(CallError::Unmatched));
        loader.serve_call(|rent: &Rent| u64::from(rent.stall) * 100);
        assert_eq!(rent.call(&Rent { stall: 4 }), Ok(400));
        assert_eq!(
            later.call(&RentLater { stall: 4 }),
            Err(CallError::Unmatched)
        );
    }

    #[test]
    fn a_described_caller_reaches_a_typed_server() {
        let loader = start();
        let (_, _, described) = callers();
        loader.serve_call(|rent: &Rent| u64::from(rent.stall) + 1);
        let mut answer = [0u8; 8];
        assert_eq!(described.call(&7u16.to_le_bytes(), &mut answer), Ok(()));
        assert_eq!(u64::from_le_bytes(answer), 8);
        assert_eq!(described.call(&[1], &mut answer), Err(CallError::Failed));
    }

    #[test]
    fn the_manifest_carries_the_served_call_s_schema() {
        // SAFETY: `enshwick_mod_query` returns the manifest of
        // `declare_mod!`, valid until its shutdown.
        let manifest = unsafe { &*enshwick_mod_query() };
        // SAFETY: the schema text lives as long as the manifest, until a shutdown no test runs.
        let text = unsafe { manifest.points.as_str() }.unwrap_or("");
        let table: toml::Table = toml::from_str(text).unwrap();
        let call = &table["call"][0];
        assert_eq!(call["name"].as_str(), Some("you.trading-post:price-of"));
        assert_eq!(call["major"].as_integer(), Some(2));
        assert_eq!(call["input"].as_str(), Some("PriceOf"));
        assert_eq!(call["output"].as_str(), Some("u32"));
        assert_eq!(call["input_size"].as_integer(), Some(8));
        assert_eq!(call["output_size"].as_integer(), Some(4));
        assert_eq!(
            call["doc"].as_str(),
            Some("What the trading post pays for an item.")
        );
        let layout: Vec<u64> = call["layout"]
            .as_array()
            .unwrap()
            .iter()
            .map(|h| h.as_str().unwrap().parse().unwrap())
            .collect();
        let hashes: Vec<u64> = PriceOf::LAYOUT.iter().map(|f| f.hash).collect();
        assert_eq!(layout, hashes);
        assert!(text.contains("name = \"regular\""), "{text}");
    }
}
