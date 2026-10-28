//! The features each mod reports on or off, for the log, the status summary
//! and the Mods screen.

use enshwick::i18n::Texts;

use crate::configure::mod_reason;

/// A mod's feature as it last reported it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Feature {
    /// The name the mod gave it.
    pub name: String,
    /// Why it is off, as the mod sent it; `None` while it runs.
    pub off: Option<String>,
}

/// Takes the report that `name` is off for `off`, or on when `off` is
/// `None`, into `features`; the log line the change asks for, if any, with
/// the reason in the reader's language from the mod's `texts`.
pub fn report(
    features: &mut Vec<Feature>,
    name: &str,
    off: Option<&str>,
    texts: Option<&Texts>,
) -> Option<String> {
    let line = |off: Option<&str>| match off {
        Some(why) => format!("feature {name} off: {}", mod_reason(texts, why)),
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

/// `features` with each reason in the reader's language from the mod's
/// `texts`, as the status summary and the Mods screen show them.
#[must_use]
pub fn shown(features: &[Feature], texts: Option<&Texts>) -> Vec<Feature> {
    features
        .iter()
        .map(|feature| Feature {
            name: feature.name.clone(),
            off: feature.off.as_deref().map(|why| mod_reason(texts, why)),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_feature_off_is_logged_and_one_on_from_the_start_is_not() {
        let mut features = Vec::new();
        assert_eq!(report(&mut features, "light", None, None), None);
        assert_eq!(
            report(
                &mut features,
                "glow",
                Some("symbol lantern.glow: missing"),
                None
            )
            .as_deref(),
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
        let _first = report(&mut features, "glow", Some("missing"), None);
        assert_eq!(report(&mut features, "glow", Some("missing"), None), None);
        assert_eq!(
            report(&mut features, "glow", None, None).as_deref(),
            Some("feature glow on")
        );
        assert_eq!(features.len(), 1);
        assert_eq!(features[0].off, None);
    }

    #[test]
    fn a_text_reason_reads_in_the_reader_s_language_and_falls_back_without_texts() {
        let texts = Texts::parse(
            "[en]\n\"glow.off\" = \"No glow: {symbol} is missing.\"\n\
             [uk]\n\"glow.off\" = \"Без сяйва: бракує {symbol}.\"\n",
        )
        .unwrap();
        let sent = "glow.off\tsymbol=lantern.glow";
        let mut features = Vec::new();
        assert_eq!(
            report(&mut features, "glow", Some(sent), Some(&texts)).as_deref(),
            Some("feature glow off: No glow: lantern.glow is missing.")
        );
        assert_eq!(features[0].off.as_deref(), Some(sent));
        let uk = Some(enshwick::i18n::Language::UK_UA);
        let shown_uk = crate::texts::for_reader(uk, || shown(&features, Some(&texts)));
        assert_eq!(
            shown_uk[0].off.as_deref(),
            Some("Без сяйва: бракує lantern.glow.")
        );
        assert_eq!(
            shown(&features, None)[0].off.as_deref(),
            Some("glow.off symbol=lantern.glow")
        );
    }
}
