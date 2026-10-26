//! A second install while this thread still holds an installed fake is a
//! mistake in the test, reported at once instead of waiting forever.

#[cfg(test)]
mod tests {
    use enshwick::test::FakeHost;

    #[test]
    #[should_panic(expected = "a fake host is already installed in this thread")]
    fn a_second_install_in_one_thread_panics() {
        let _first = FakeHost::new().install();
        let _second = FakeHost::new().install();
    }

    #[test]
    fn an_install_after_the_first_is_dropped_goes_ahead() {
        drop(FakeHost::new().install());
        drop(FakeHost::new().install());
    }
}
