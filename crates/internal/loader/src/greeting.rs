//! The lines a player reads in chat on joining a server: the warning that
//! developer tools are on, then what the platform turned off.

/// What a player reads, in order: the developer tools warning `developer`,
/// then the line naming what is off.
#[must_use]
pub(crate) fn lines(developer: Option<String>, off: Option<String>) -> Vec<String> {
    developer.into_iter().chain(off).collect()
}

/// What the loaded game greets a player with.
#[cfg(windows)]
#[must_use]
pub(crate) fn of(loader: &crate::host::Loader) -> Vec<String> {
    let developer = loader
        .developer_tools
        .then(|| crate::developer_tools::warning().to_owned());
    let off = crate::off_line::line(&crate::off_line::names(loader), &crate::off_line::takers());
    lines(developer, off)
}

/// Greets the player with account key `key`, who just joined this server.
#[cfg(windows)]
pub(crate) fn joined(loader: &crate::host::Loader, key: u64) {
    for line in of(loader) {
        let _sent = crate::chat::send(key, &line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_warning_comes_first_then_what_is_off() {
        assert_eq!(
            lines(Some("tools".into()), Some("off".into())),
            ["tools", "off"]
        );
        assert_eq!(lines(None, Some("off".into())), ["off"]);
    }

    #[test]
    fn nothing_to_say_is_no_line() {
        assert!(lines(None, None).is_empty());
    }
}
