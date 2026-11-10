//! The check that a mod keeps unwinding on panic.

/// Refuses to compile a mod built without `panic = "unwind"`: the loader
/// catches a mod's panics to turn off only that mod, which an aborting mod
/// would turn into a crash of the game.
#[doc(hidden)]
#[macro_export]
macro_rules! __require_unwind {
    () => {
        #[cfg(not(panic = "unwind"))]
        ::core::compile_error!(
            "an Enshwick mod must be built with `panic = \"unwind\"`: remove `panic = \"abort\"` from its Cargo profile"
        );
    };
}
