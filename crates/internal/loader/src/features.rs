//! The features each mod reports on or off, for the log, the status summary
//! and the Mods screen.

/// A mod's feature as it last reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature {
    /// The name the mod gave it.
    pub name: String,
    /// Why it is off; `None` while it runs.
    pub off: Option<String>,
}

/// Takes the report that `name` is off for `off`, or on when `off` is
/// `None`, into `features`; the log line the change asks for, if any.
pub fn report(features: &mut Vec<Feature>, name: &str, off: Option<&str>) -> Option<String> {
    let line = |off: Option<&str>| match off {
        Some(why) => format!("feature {name} off: {why}"),
        None => format!("feature {name} on"),
    };
    match features.iter_mut().find(|f| f.name == name) {
        Some(kept) if kept.off.as_deref() == off => None,
        Some(kept) => {
            kept.off = off.map(str::to_owned);
            Some(line(off))
        }
        None => {
            features.push(Feature {
                name: name.to_owned(),
                off: off.map(str::to_owned),
            });
            off.is_some().then(|| line(off))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feature_off_is_logged_and_one_on_from_the_start_is_not() {
        let mut features = Vec::new();
        assert_eq!(report(&mut features, "light", None), None);
        assert_eq!(
            report(&mut features, "glow", Some("symbol lantern.glow: missing")).as_deref(),
            Some("feature glow off: symbol lantern.glow: missing")
        );
        assert_eq!(features.len(), 2);
        assert_eq!(
            features[1].off.as_deref(),
            Some("symbol lantern.glow: missing")
        );
    }

    #[test]
    fn a_report_that_changes_nothing_logs_nothing() {
        let mut features = Vec::new();
        let _first = report(&mut features, "glow", Some("missing"));
        assert_eq!(report(&mut features, "glow", Some("missing")), None);
        assert_eq!(
            report(&mut features, "glow", None).as_deref(),
            Some("feature glow on")
        );
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].off, None);
    }
}
