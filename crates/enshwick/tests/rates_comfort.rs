//! A comfort rate scales what furniture registers after the mod's filters.

#[cfg(test)]
mod tests {
    use enshwick::abi::{Comfort, Header, Status};
    use enshwick::rates::Rate;
    use enshwick::test::FakeHost;

    struct Cosy;

    impl enshwick::Mod for Cosy {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.cosy", side: SERVER)
        }

        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            load.on_comfort(|provider| provider.value + 1)?;
            load.rate(Rate::Comfort)?.set(1.5)?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Cosy);

    #[test]
    fn comfort_takes_the_filters_then_the_rate_rounded() {
        let loader = FakeHost::new().install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Cosy)`.
        assert_eq!(unsafe { loader.init(enshwick_mod_init) }, Status::OK);
        let comfort = Comfort {
            header: Header::of::<Comfort>(),
            category: 1,
            value: 2,
            position: [0.0; 3],
        };
        assert_eq!(loader.comfort(&comfort), 5, "(2 + 1) x 1.5 rounds to 5");
    }
}
