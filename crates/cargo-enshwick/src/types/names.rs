//! The game's names in the casing each language wants.

/// Rust's keywords, which a name takes as a raw identifier.
const RUST_KEYWORDS: &[&str] = &[
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "do", "dyn",
    "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl", "in", "let",
    "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref", "return",
    "static", "struct", "trait", "true", "try", "type", "typeof", "unsafe", "unsized", "use",
    "virtual", "where", "while", "yield",
];
/// Keywords a raw identifier cannot carry.
const NOT_RAW: &[&str] = &["crate", "self", "Self", "super"];
/// Luau's keywords, which a field name cannot be bare.
const LUAU_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "if", "in", "local",
    "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// The namespaces and the last name of a full name, split at `::` outside
/// template brackets: `keen::ecs::Health` is `(["keen", "ecs"], "Health")`.
pub(crate) fn split(full: &str) -> (Vec<&str>, &str) {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0usize, 0);
    let bytes = full.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => depth = depth.saturating_sub(1),
            b':' if depth == 0 && bytes.get(i + 1) == Some(&b':') => {
                parts.push(&full[start..i]);
                start = i + 2;
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    (parts, &full[start..])
}

/// The words of a name: split at characters that are no letter or digit,
/// and where a capital starts a word, as in `rangeX` or `HTTPServer`.
fn words(name: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for piece in name.split(|c: char| !c.is_ascii_alphanumeric()) {
        let chars: Vec<char> = piece.chars().collect();
        let mut word = String::new();
        for (i, &c) in chars.iter().enumerate() {
            let previous = i.checked_sub(1).map(|j| chars[j]);
            let next = chars.get(i + 1);
            let starts = c.is_ascii_uppercase()
                && previous.is_some_and(|p| {
                    p.is_ascii_lowercase()
                        || p.is_ascii_digit()
                        || (p.is_ascii_uppercase() && next.is_some_and(char::is_ascii_lowercase))
                });
            if starts && !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
            word.push(c);
        }
        if !word.is_empty() {
            words.push(word);
        }
    }
    words
}

/// `name` as a Rust field or module name: `rangeX` is `range_x`, a keyword
/// a raw identifier.
pub(crate) fn snake(name: &str) -> String {
    let joined = words(name)
        .iter()
        .map(|w| w.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("_");
    rust_ident(joined)
}

/// `name` as a Rust constant: `MagicalFog` is `MAGICAL_FOG`.
pub(crate) fn shout(name: &str) -> String {
    let joined = words(name)
        .iter()
        .map(|w| w.to_ascii_uppercase())
        .collect::<Vec<_>>()
        .join("_");
    rust_ident(joined)
}

/// `name` as a Rust type: each piece between characters that are no
/// letter or digit starts with a capital and the pieces join, as in
/// `uint` to `Uint` or a template's name; an underscore stays only between
/// two digits, as in `f32_4x4` to `F32_4x4`.
pub(crate) fn camel(name: &str) -> String {
    let mut joined = String::new();
    for piece in name.split(|c: char| !c.is_ascii_alphanumeric()) {
        let mut chars = piece.chars();
        let Some(first) = chars.next() else {
            continue;
        };
        if joined.ends_with(|c: char| c.is_ascii_digit()) && first.is_ascii_digit() {
            joined.push('_');
        }
        joined.push(first.to_ascii_uppercase());
        joined.push_str(chars.as_str());
    }
    rust_ident(joined)
}

/// `name` made a usable Rust identifier: never empty, never starting with a
/// digit, a keyword raw or suffixed.
fn rust_ident(mut name: String) -> String {
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        name.insert(0, '_');
    }
    if NOT_RAW.contains(&name.as_str()) {
        name.push('_');
    } else if RUST_KEYWORDS.contains(&name.as_str()) {
        name.insert_str(0, "r#");
    }
    name
}

/// Whether `name` is a bare Luau field name.
pub(crate) fn is_luau_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !LUAU_KEYWORDS.contains(&name)
}

/// `name` with `_2`, `_3` and on appended until `taken` does not hold it.
pub(crate) fn unique(name: String, taken: &mut std::collections::BTreeSet<String>) -> String {
    if !taken.contains(&name) {
        taken.insert(name.clone());
        return name;
    }
    let mut n = 2;
    loop {
        let candidate = format!("{name}_{n}");
        if taken.insert(candidate.clone()) {
            return candidate;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_name_splits_outside_templates() {
        assert_eq!(split("keen::ecs::Health"), (vec!["keen", "ecs"], "Health"));
        assert_eq!(
            split("keen::Pair<keen::ecs::A,3>"),
            (vec!["keen"], "Pair<keen::ecs::A,3>")
        );
        assert_eq!(split("Plain"), (vec![], "Plain"));
    }

    #[test]
    fn names_take_each_language_s_casing() {
        assert_eq!(snake("rangeX"), "range_x");
        assert_eq!(snake("HTTPServer"), "http_server");
        assert_eq!(snake("type"), "r#type");
        assert_eq!(snake("self"), "self_");
        assert_eq!(snake("2d"), "_2d");
        assert_eq!(shout("MagicalFog"), "MAGICAL_FOG");
        assert_eq!(shout("Invalid"), "INVALID");
        assert_eq!(camel("Health"), "Health");
        assert_eq!(camel("Pair<keen::ecs::A,3>"), "PairKeenEcsA3");
        assert_eq!(camel("SInt32Attribute"), "SInt32Attribute");
        assert_eq!(camel("f32"), "F32", "never a primitive's name");
        assert_eq!(camel("f32_4x4"), "F32_4x4");
        assert_eq!(
            camel("BlueprintMaterial_Default"),
            "BlueprintMaterialDefault"
        );
    }

    #[test]
    fn a_taken_name_gets_a_number() {
        let mut taken = std::collections::BTreeSet::new();
        assert_eq!(unique("a".into(), &mut taken), "a");
        assert_eq!(unique("a".into(), &mut taken), "a_2");
        assert_eq!(unique("a".into(), &mut taken), "a_3");
    }

    #[test]
    fn luau_field_names_are_checked() {
        assert!(is_luau_name("rangeX"));
        assert!(!is_luau_name("end"));
        assert!(!is_luau_name("a-b"));
    }
}
