//! What the internal crates widened to `pub` for each other stays out of a
//! mod's reach.

#[cfg(test)]
mod tests {
    #[test]
    fn a_mod_reaches_no_internal_call_and_makes_no_loading_of_its_own() {
        trybuild::TestCases::new().compile_fail("tests/ui/internals_out_of_reach.rs");
    }
}
