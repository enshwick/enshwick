//! Why settings did not read, told without any value of them.

/// `error`, from reading the settings `text` when it is at hand, as a
/// reason that holds no value of the text.
///
/// It names the line, the key path and what was expected; a syntax error
/// keeps the parser's reason, which holds no text of the file.
pub(crate) fn reason(text: Option<&str>, error: &toml::de::Error) -> String {
    let line = text.zip(error.span()).map(|(text, span)| {
        let bytes = text.as_bytes();
        bytes
            .get(..span.start)
            .unwrap_or(bytes)
            .split(|&b| b == b'\n')
            .count()
    });
    let why = if text.is_some_and(|text| text.parse::<toml::Table>().is_err()) {
        error.message().trim_end().to_owned()
    } else {
        expected(error.message())
    };
    match (line, key_path(error)) {
        (Some(line), Some(key)) => format!("line {line}, key {key}: {why}"),
        (Some(line), None) => format!("line {line}: {why}"),
        (None, Some(key)) => format!("key {key}: {why}"),
        (None, None) => why,
    }
}

/// The dotted key `error` happened under, which toml shows only in its text.
fn key_path(error: &toml::de::Error) -> Option<String> {
    let mut bare = error.clone();
    bare.set_input(None);
    let shown = bare.to_string();
    let path = shown
        .strip_prefix(error.message())?
        .strip_prefix("\nin `")?
        .strip_suffix("`\n")?;
    (!path.is_empty()).then(|| path.to_owned())
}

/// What serde's `message` says was expected, without the value it found.
fn expected(message: &str) -> String {
    const FOUND_A_VALUE: [&str; 4] = [
        "invalid type: ",
        "invalid value: ",
        "invalid length ",
        "unknown variant `",
    ];
    const NAMED_A_KEY: [&str; 3] = ["missing field `", "unknown field `", "duplicate field `"];
    if FOUND_A_VALUE.iter().any(|start| message.starts_with(start))
        && let Some((_, wanted)) = message.rsplit_once(", expected ")
    {
        return format!("expected {}", wanted.trim_end());
    }
    if NAMED_A_KEY.iter().any(|start| message.starts_with(start)) {
        return message.trim_end().to_owned();
    }
    "the value does not fit its type".to_owned()
}

#[cfg(test)]
mod tests {
    use super::reason;

    #[derive(Debug, serde::Deserialize)]
    #[expect(dead_code, reason = "only read to fail")]
    struct Vault {
        pin: u32,
        inner: Inner,
    }

    #[derive(Debug, serde::Deserialize)]
    #[expect(dead_code, reason = "only read to fail")]
    struct Inner {
        token: String,
    }

    fn reason_for(text: &str) -> String {
        let error = toml::from_str::<Vault>(text).unwrap_err();
        let told = reason(Some(text), &error);
        assert!(
            !told.contains("hunter2") && !told.contains("4815162342"),
            "{told}"
        );
        told
    }

    #[test]
    fn values_never_reach_the_reason() {
        assert_eq!(
            reason_for("pin = \"hunter2\"\n[inner]\ntoken = \"x\"\n"),
            "line 1, key pin: expected u32"
        );
        assert_eq!(
            reason_for("pin = 1\n[inner]\ntoken = 4815162342\n"),
            "line 3, key inner.token: expected a string"
        );
        assert_eq!(
            reason_for("pin = 1\ninner = { token = 4815162342 }\n"),
            "line 2, key inner.token: expected a string"
        );
        assert_eq!(reason_for("pin = 1\n"), "line 1: missing field `inner`");
        assert!(reason_for("pin = 1\ntoken = \"hunter2\n").starts_with("line 2: "));
        let value: toml::Value = toml::from_str("pin = \"hunter2\"").unwrap();
        let error = value.try_into::<Vault>().unwrap_err();
        let told = reason(None, &error);
        assert!(
            told.ends_with("expected u32") && !told.contains("hunter2"),
            "{told}"
        );
    }
}
