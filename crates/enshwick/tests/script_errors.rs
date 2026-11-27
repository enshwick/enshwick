//! A mod's `#[script]` method returns any error of the API or a script
//! error of its own, and an error without a kind does not compile.

#[cfg(test)]
mod tests {
    use enshwick::inventory::RunBatchError;
    use enshwick::script::Error;

    /// A mod's object with script methods: the gifts it has left.
    struct Gifts {
        left: u32,
    }

    #[enshwick::script(gifts)]
    impl Gifts {
        /// Gives a gift, failing as the API's batch run does when none is left.
        #[script]
        pub(crate) fn give(&self) -> Result<u32, RunBatchError> {
            self.left.checked_sub(1).ok_or(RunBatchError::Empty)
        }

        /// Rolls up to `sides` gifts, failing with a kind the mod names.
        #[script]
        pub(crate) fn roll(&self, sides: u32) -> Result<u32, Error> {
            if sides == 0 {
                Err(Error::with_kind(Error::BAD_ARGS, "sides: at least 1"))
            } else {
                Ok(sides.min(self.left))
            }
        }
    }

    #[test]
    fn a_script_method_returns_an_api_error_or_a_script_error_of_its_own() {
        assert_eq!(Gifts::SCRIPT_GIFTS.len(), 2);
        let none = Gifts { left: 0 };
        assert_eq!(none.give(), Err(RunBatchError::Empty));
        assert_eq!(
            none.roll(0).map_err(|e| e.kind().to_owned()),
            Err("BadArgs".to_owned())
        );
    }

    #[test]
    fn an_error_without_a_kind_is_no_script_methods_error() {
        trybuild::TestCases::new().compile_fail("tests/ui/script_error_without_kind.rs");
    }
}
