//! Every typed game event and each of its fields carry the name of the
//! engine type or field their catalogue id stands for; the catalogue's own
//! test holds each id to the engine name, so the API cannot drift from the
//! game's names.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    /// Fields enshwick computes itself, which keep enshwick's names.
    const OURS: &[&str] = &["block", "killer", "killer_player"];

    /// `name` in lower case without underscores or a raw identifier prefix,
    /// to compare a Rust name with an id.
    fn plain(name: &str) -> String {
        name.trim_start_matches("r#")
            .chars()
            .filter(|&c| c != '_')
            .map(|c| c.to_ascii_lowercase())
            .collect()
    }

    fn source(file: &str) -> String {
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(file)).unwrap()
    }

    /// Each id constant of the API's id table with its id.
    fn ids() -> BTreeMap<String, String> {
        source("src/engine_ids.rs")
            .lines()
            .filter_map(|line| {
                let (name, rest) = line.trim().split_once(" = \"")?;
                Some((name.to_owned(), rest.strip_suffix("\";")?.to_owned()))
            })
            .collect()
    }

    /// The ids `text` names as `ids::NAME`.
    fn named<'a>(text: &str, ids: &'a BTreeMap<String, String>) -> Vec<&'a str> {
        text.split("ids::")
            .skip(1)
            .filter_map(|rest| {
                let end = rest
                    .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
                    .unwrap_or(rest.len());
                ids.get(&rest[..end]).map(String::as_str)
            })
            .collect()
    }

    /// Each public struct of `text` with its public fields.
    fn structs(text: &str) -> Vec<(String, Vec<String>)> {
        text.split("\npub struct ")
            .skip(1)
            .map(|rest| {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                let body = &rest[..rest.find("\n}").unwrap_or(rest.len())];
                let fields = body
                    .lines()
                    .filter_map(|line| line.trim().strip_prefix("pub "))
                    .filter_map(|line| line.split_once(':').map(|(field, _)| field.to_owned()))
                    .collect();
                (name, fields)
            })
            .collect()
    }

    #[test]
    fn every_typed_event_and_field_takes_the_game_s_name() {
        let text = source("src/game_events/kinds.rs");
        let ids = ids();
        let all_named = named(&text, &ids);
        for (name, fields) in structs(&text) {
            let marker = format!("impl Kind for {name} {{");
            let Some(start) = text.find(&marker) else {
                for field in fields.iter().filter(|f| !OURS.contains(&f.as_str())) {
                    let suffixes = all_named.iter().filter_map(|id| id.split_once('.'));
                    assert!(
                        suffixes.map(|(_, s)| plain(s)).any(|s| s == plain(field)),
                        "{name}.{field} names no field of the game"
                    );
                }
                continue;
            };
            let body = &text[start..];
            let body = &body[..body[1..].find("\nimpl ").map_or(body.len(), |end| end + 1)];
            let used = named(body, &ids);
            let event = used
                .iter()
                .find(|id| !id.contains('.'))
                .unwrap_or_else(|| panic!("{name} resolves no event type"));
            assert_eq!(plain(&name), plain(event), "{name}");
            for field in fields.iter().filter(|f| !OURS.contains(&f.as_str())) {
                let found = used
                    .iter()
                    .filter_map(|id| id.strip_prefix(&format!("{event}.")))
                    .any(|suffix| plain(suffix) == plain(field));
                assert!(found, "{name}.{field} reads no field of that name");
            }
        }
    }

    #[test]
    fn a_struct_s_public_fields_are_read_from_its_body() {
        let text = "/// A.\npub struct Hit {\n    /// B.\n    pub root_source_id: u32,\n    pub r#type: u8,\n}\n";
        let found = structs(&format!("\n{text}"));
        assert_eq!(
            found,
            [(
                "Hit".to_owned(),
                vec!["root_source_id".to_owned(), "r#type".to_owned()]
            )]
        );
        assert_eq!(plain("r#type"), "type");
    }
}
