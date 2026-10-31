//! A link described at run time, as a script's schema gives it, serves the
//! owner's point like a typed link, and a wrong layout is refused.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::points::{
        Decision, DescribedField, DescribedPoint, DescribedVerdict, Mode, Order, Point,
    };
    use enshwick::{Load, Mod, Off};

    #[enshwick::point(name = "you.trading-post:price", mode = Override, output = u32, refusable)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Price {
        pub(crate) item: u32,
        pub(crate) count: u32,
        pub(crate) player: u64,
    }

    fn described() -> DescribedPoint {
        let input = Price::LAYOUT
            .iter()
            .map(|f| {
                // SAFETY: the point macro's layout names are static strings.
                let name = unsafe { f.name.as_str() }.unwrap_or("");
                DescribedField::new(name, f.hash)
            })
            .collect();
        DescribedPoint::new(Price::NAME, Mode::Override)
            .refusable(true)
            .with_input(input, u32::try_from(size_of::<Price>()).unwrap())
            .with_output(Price::OUTPUT[0].hash, 4)
    }

    static POINT: std::sync::Mutex<Option<DescribedPoint>> = std::sync::Mutex::new(None);

    struct Haggler;

    impl Mod for Haggler {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            // SAFETY: every answer set is a `u32`, the point's answer type.
            unsafe {
                load.extend_described_point(
                    &POINT.lock().unwrap().take().unwrap_or_else(described),
                    &Order::new(),
                    |input, answer| {
                        let item = u32::from_le_bytes(input[..4].try_into().unwrap_or_default());
                        let price = u32::from_le_bytes(answer.try_into().unwrap_or_default());
                        match item {
                            7 => DescribedVerdict::Set((price / 2).to_le_bytes().to_vec()),
                            9 => DescribedVerdict::Refuse,
                            _ => DescribedVerdict::Pass,
                        }
                    },
                )
            }?;
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Haggler,
        r#"
        id = "you.haggler"
        side = "server"
        "#
    );

    fn start(point: DescribedPoint) -> enshwick::test::Installed {
        // After the install, which tests take in turn: set earlier, another
        // test's mod could take this one's point.
        let loader = enshwick::test::FakeHost::new().install();
        *POINT.lock().unwrap() = Some(point);
        // SAFETY: both functions are the exports of this file's `declare_mod!(Haggler)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn serves_nothing(point: DescribedPoint) {
        let loader = start(point);
        assert_eq!(
            loader.ask_point(
                &Price {
                    item: 7,
                    count: 1,
                    player: 1
                },
                10
            ),
            Decision::Kept(10)
        );
    }

    #[test]
    fn a_wrong_layout_hash_serves_nothing() {
        let mut wrong = described();
        wrong.input[0].hash ^= 1;
        serves_nothing(wrong);
    }

    #[test]
    fn an_input_larger_than_the_owner_s_serves_nothing() {
        serves_nothing(described().with_input(described().input, 64));
    }

    #[test]
    fn an_answer_of_another_size_serves_nothing() {
        serves_nothing(described().with_output(Price::OUTPUT[0].hash, 8));
    }

    #[test]
    fn a_described_link_serves_the_typed_point() {
        let loader = enshwick::test::FakeHost::new().install();
        *POINT.lock().unwrap() = None;
        // SAFETY: both functions are the exports of this file's `declare_mod!(Haggler)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        assert_eq!(loader.attached_points(), ["you.trading-post:price"]);
        let ask = |item| {
            loader.ask_point(
                &Price {
                    item,
                    count: 1,
                    player: 1,
                },
                10,
            )
        };
        assert_eq!(ask(7), Decision::Changed(5));
        assert_eq!(ask(9), Decision::Refused);
        assert_eq!(ask(1), Decision::Kept(10));
    }
}
