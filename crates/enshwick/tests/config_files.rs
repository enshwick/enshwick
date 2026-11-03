//! Two settings types of one mod that write one file fail its build.

#[cfg(test)]
mod tests {
    #[test]
    fn two_settings_types_writing_one_file_fail_the_build_naming_both() {
        let cases = trybuild::TestCases::new();
        cases.compile_fail("tests/ui/two_settings_types_one_file.rs");
        cases.compile_fail("tests/ui/group_not_declared.rs");
        cases.pass("tests/ui/settings_types_with_files_and_a_group.rs");
    }
}
