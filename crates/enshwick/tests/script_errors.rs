//! A mod's `#[script]` method returns any error of the API or a script
//! error of its own, and an error without a kind does not compile.

use enshwick::inventory::RunBatchError;
use enshwick::script::Error;

/// A mod's object with script methods.
struct Gifts;

#[enshwick::script(gifts)]
impl Gifts {
    /// Fails as the API's batch run does.
    #[script]
    pub(crate) fn give(&self) -> Result<u32, RunBatchError> {
        Err(RunBatchError::Empty)
    }

    /// Fails with a kind the mod names.
    #[script]
    pub(crate) fn roll(&self, sides: u32) -> Result<u32, Error> {
        if sides == 0 {
            Err(Error::with_kind(Error::BAD_ARGS, "sides: at least 1"))
        } else {
            Ok(sides)
        }
    }
}

#[test]
fn a_script_method_returns_an_api_error_or_a_script_error_of_its_own() {
    assert_eq!(Gifts::SCRIPT_GIFTS.len(), 2);
}

#[test]
fn an_error_without_a_kind_is_no_script_methods_error() {
    trybuild::TestCases::new().compile_fail("tests/ui/script_error_without_kind.rs");
}
