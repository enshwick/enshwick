//! Reasons given as a text of a mod's texts, as they reach the loader: the
//! key, then `name=value` pairs, tab-separated, on one line.

/// `key` and `args` as the loader reads them back with
/// [`Texts::reason`](super::Texts::reason).
pub(crate) fn encode<N: AsRef<str>, V: AsRef<str>>(key: &str, args: &[(N, V)]) -> String {
    let mut reason = key.replace(SEPARATORS, " ");
    for (name, value) in args {
        reason.push('\t');
        reason.push_str(&name.as_ref().replace(SEPARATORS, " ").replace('=', " "));
        reason.push('=');
        reason.push_str(&value.as_ref().replace(SEPARATORS, " "));
    }
    reason
}

/// `key` and `args` as a log shows them: `key (name = value, ...)`.
pub(crate) fn show<N: AsRef<str>, V: AsRef<str>>(key: &str, args: &[(N, V)]) -> String {
    if args.is_empty() {
        return key.to_owned();
    }
    let shown: Vec<String> = args
        .iter()
        .map(|(name, value)| format!("{} = {}", name.as_ref(), value.as_ref()))
        .collect();
    format!("{key} ({})", shown.join(", "))
}

/// The key of `reason` and its `name=value` pairs; a reason given as plain
/// words reads as a key no text has.
pub(crate) fn decode(reason: &str) -> (&str, Vec<(&str, &str)>) {
    let mut parts = reason.split('\t');
    let key = parts.next().unwrap_or_default();
    (key, parts.filter_map(|pair| pair.split_once('=')).collect())
}

/// What ends a part or the line.
const SEPARATORS: [char; 3] = ['\t', '\n', '\r'];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_and_its_values_read_back_as_given() {
        let reason = encode(
            "glow.missing",
            &[("symbol", "lantern.glow"), ("build", "426")],
        );
        assert_eq!(reason, "glow.missing\tsymbol=lantern.glow\tbuild=426");
        assert_eq!(
            decode(&reason),
            (
                "glow.missing",
                vec![("symbol", "lantern.glow"), ("build", "426")]
            )
        );
        assert_eq!(decode(&encode::<&str, &str>("off", &[])), ("off", vec![]));
    }

    #[test]
    fn separators_in_a_key_name_or_value_become_spaces() {
        let reason = encode("a\nkey", &[("of\t=x", "a\tb\r\nc")]);
        assert_eq!(reason, "a key\tof  x=a b  c");
        assert_eq!(decode(&reason), ("a key", vec![("of  x", "a b  c")]));
    }

    #[test]
    fn a_value_may_hold_an_equals_sign() {
        assert_eq!(
            decode(&encode("sum", &[("is", "1+1=2")])),
            ("sum", vec![("is", "1+1=2")])
        );
    }

    #[test]
    fn a_log_shows_the_key_and_its_values() {
        assert_eq!(
            show("stalls.min", &[("min", "2"), ("of", "3")]),
            "stalls.min (min = 2, of = 3)"
        );
        assert_eq!(show::<&str, &str>("stalls.off", &[]), "stalls.off");
    }

    #[test]
    fn plain_words_read_as_a_key_without_values() {
        assert_eq!(decode("limit is 1 to 16"), ("limit is 1 to 16", vec![]));
    }
}
