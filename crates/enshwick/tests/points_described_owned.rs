//! A point defined from its description hands typed links aligned values,
//! even when the owner asks with bytes at any address.

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, PoisonError};

    use enshwick::abi::Status;
    use enshwick::points::{
        Decision, DescribedField, DescribedOwnedPoint, DescribedPoint, Mode, Point, Verdict,
    };
    use enshwick::{Load, Mod, Off};

    #[enshwick::point(name = "you.stall:rent", mode = Override, output = u64, refusable)]
    #[derive(Clone, Copy, Debug)]
    pub(crate) struct Rent {
        pub(crate) stall: u64,
    }

    /// Bytes at a 16-byte boundary, so the slice after the first is never
    /// aligned for a `u64`.
    #[repr(C, align(16))]
    struct Bytes([u8; 9]);

    static RENT: Mutex<Option<DescribedOwnedPoint>> = Mutex::new(None);

    struct Stall;

    impl Mod for Stall {
        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            // SAFETY: the point macro's layout names are static strings.
            let name = unsafe { Rent::LAYOUT[0].name.as_str() }.unwrap_or("");
            let point = DescribedPoint::new(Rent::NAME, Mode::Override)
                .refusable(true)
                .with_input(vec![DescribedField::new(name, Rent::LAYOUT[0].hash)], 8)
                .with_output(Rent::OUTPUT[0].hash, 8);
            // SAFETY: every input and answer asked is a `u64`, valid for any bytes.
            let rent = unsafe { load.define_described_point(&point) }?;
            *RENT.lock().unwrap_or_else(PoisonError::into_inner) = Some(rent);
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Stall,
        r#"
        id = "you.stall"
        side = "server"
        "#
    );

    fn started() -> enshwick::test::Installed {
        let loader = enshwick::test::FakeHost::new().install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Stall)`.
        let status =
            unsafe { loader.start(enshwick_mod_query, enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        loader
    }

    fn rent() -> DescribedOwnedPoint {
        RENT.lock().unwrap().unwrap()
    }

    #[test]
    fn without_a_link_or_with_bytes_of_another_size_the_answer_stays() {
        let loader = started();
        let mut answer = 10u64.to_le_bytes();
        assert!(!rent().is_extended());
        assert_eq!(
            rent().decide(&3u64.to_le_bytes(), &mut answer),
            Decision::Kept(())
        );
        assert!(loader.extend_point(|_: &Rent, _: &u64| Verdict::Set(1)));
        assert!(rent().is_extended());
        assert_eq!(rent().decide(&[0; 4], &mut answer), Decision::Kept(()));
        assert_eq!(
            rent().decide(&3u64.to_le_bytes(), &mut [0; 4]),
            Decision::Kept(())
        );
        assert_eq!(u64::from_le_bytes(answer), 10);
        assert_eq!(
            rent().decide(&3u64.to_le_bytes(), &mut answer),
            Decision::Changed(())
        );
        assert_eq!(u64::from_le_bytes(answer), 1);
    }

    #[test]
    fn a_link_that_passes_keeps_and_one_that_refuses_refuses() {
        let loader = started();
        assert!(loader.extend_point(|rent: &Rent, _: &u64| {
            if rent.stall == 9 {
                Verdict::Refuse
            } else {
                Verdict::Pass
            }
        }));
        let mut answer = 10u64.to_le_bytes();
        assert_eq!(
            rent().decide(&1u64.to_le_bytes(), &mut answer),
            Decision::Kept(())
        );
        assert_eq!(
            rent().decide(&9u64.to_le_bytes(), &mut answer),
            Decision::Refused
        );
    }

    #[test]
    fn a_typed_link_reads_bytes_asked_at_an_odd_address_aligned() {
        let loader = started();
        assert!(loader.extend_point(|rent: &Rent, answer: &u64| {
            let aligned = core::ptr::from_ref(rent)
                .addr()
                .is_multiple_of(align_of::<Rent>())
                && core::ptr::from_ref(answer)
                    .addr()
                    .is_multiple_of(align_of::<u64>());
            if aligned {
                Verdict::Set(answer + rent.stall)
            } else {
                Verdict::Pass
            }
        }));
        let rent = RENT.lock().unwrap().unwrap();
        let mut input = Bytes([0u8; 9]);
        input.0[1..].copy_from_slice(&3u64.to_le_bytes());
        let mut answer = Bytes([0u8; 9]);
        answer.0[1..].copy_from_slice(&10u64.to_le_bytes());
        assert_eq!(
            rent.decide(&input.0[1..], &mut answer.0[1..]),
            Decision::Changed(())
        );
        assert_eq!(u64::from_le_bytes(answer.0[1..].try_into().unwrap()), 13);
    }
}
