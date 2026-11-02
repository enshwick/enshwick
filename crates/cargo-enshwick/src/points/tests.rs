use super::*;

const SCHEMA: &str = include_str!("../../tests/fixtures/lockers.points.toml");

#[test]
fn a_point_becomes_a_declaration_of_the_alias_with_its_types_and_layout_checks() {
    let schema = Schema::parse(SCHEMA).unwrap();
    let code = rust_deps(&schema, "lockers").unwrap();
    assert_eq!(code, include_str!("../../tests/fixtures/lockers_points.rs"));
}

#[test]
fn the_examples_declarations_are_what_the_trading_post_publishes() {
    let schema = Schema::parse(include_str!(
        "../../../../examples/trading-post/points.toml"
    ))
    .unwrap();
    assert_eq!(
        rust_deps(&schema, "trading_post").unwrap(),
        include_str!("../../../../examples/haggling/src/deps/trading_post.rs"),
        "run cargo enshwick deps trading_post again"
    );
}

#[test]
fn a_doc_keeps_its_first_sentence_as_a_paragraph_of_its_own() {
    let mut code = String::new();
    doc(&mut code, "    ", "The price. A refusal means no deal.", "");
    assert_eq!(
        code,
        "    /// The price.\n    ///\n    /// A refusal means no deal.\n"
    );
    let mut code = String::new();
    doc(&mut code, "", "", "The input.");
    assert_eq!(code, "/// The input.\n");
}

#[test]
fn a_doc_can_open_no_code_block_and_hides_nothing() {
    let hostile = [
        "The character.\n\n```\nstd::fs::write(\"x\", \"\").ok();\n```",
        "The deal.\n\n    std::process::Command::new(\"true\").status().ok();",
        "The deal. ~~~ std::fs::write(\"x\", \"\").ok();",
        "The deal. ```rust",
        "The \u{202e}deal\u{200b}.",
    ];
    for text in hostile {
        let mut code = String::new();
        doc(&mut code, "", text, "");
        for line in code.lines() {
            let body = line.strip_prefix("/// ").unwrap_or_default();
            assert!(
                !body.starts_with([' ']) && !body.starts_with("```") && !body.starts_with("~~~"),
                "{line}"
            );
            assert!(!body.chars().any(is_hidden), "{line:?}");
        }
        assert!(code.lines().count() <= 3, "{code}");
    }
}

#[test]
fn a_schema_value_that_would_leave_its_place_in_the_code_is_refused() {
    let hostile = [
        (
            "mod = \"you.lockers\"",
            "mod = \"you.lockers\\nfn evil() {}\"",
        ),
        ("version = \"1.2.0\"", "version = \"1.2.0 }\""),
        (
            "name = \"you.lockers:price\"",
            "name = \"you.lockers:price\\nfn evil() {}\"",
        ),
        ("input = \"Price\"", "input = \"Price {}\""),
        ("output = \"u32\"", "output = \"u32> evil\""),
        (
            "name = \"character\"",
            "name = \"character: u8, } fn evil() { let x\"",
        ),
        ("type = \"bool\"", "type = \"bool, } fn evil() {}\""),
        ("repr = \"u32\"", "repr = \"u32)] fn evil() {} #[allow(\""),
        (
            "name = \"BUY\"",
            "name = \"BUY: Self = Self(0); fn evil() {} const X\"",
        ),
    ];
    for (good, bad) in hostile {
        assert!(SCHEMA.contains(good), "{good}");
        let text = SCHEMA.replacen(good, bad, 1);
        assert!(
            matches!(Schema::parse(&text), Err(Error::Invalid { .. })),
            "{bad}"
        );
    }
}

#[test]
fn a_schema_name_that_rust_keeps_for_itself_is_refused() {
    let keywords = [
        ("name = \"BUY\"", "name = \"while\""),
        ("name = \"character\"", "name = \"type\""),
        ("name = \"character\"", "name = \"self\""),
        ("input = \"Price\"", "input = \"Self\""),
        ("name = \"BUY\"", "name = \"_\""),
        ("name = \"BUY\"", "name = \"async\""),
    ];
    for (good, bad) in keywords {
        assert!(SCHEMA.contains(good), "{good}");
        let text = SCHEMA.replacen(good, bad, 1);
        assert!(
            matches!(Schema::parse(&text), Err(Error::Invalid { .. })),
            "{bad}"
        );
    }
}
#[test]
fn a_quote_in_a_point_name_stays_inside_its_string() {
    let text = SCHEMA.replace("you.lockers:price", "you.lockers:price\\\", evil = (\\\"");
    let schema = Schema::parse(&text).unwrap();
    let code = rust_deps(&schema, "lockers").unwrap();
    assert!(
        code.contains(r#"name = "price\", evil = (\"", of = lockers"#),
        "{code}"
    );
}

#[test]
fn an_alias_that_is_no_rust_name_is_refused() {
    let schema = Schema::parse(SCHEMA).unwrap();
    for alias in ["", "two words", "lockers::evil", "self", "9lives"] {
        assert!(
            matches!(
                rust_deps(&schema, alias),
                Err(Error::Invalid { what: "alias", .. })
            ),
            "{alias:?}"
        );
    }
}

#[test]
fn control_characters_in_a_doc_are_dropped() {
    let mut code = String::new();
    doc(&mut code, "", "A price\rpub fn evil() {}\u{7}.", "");
    assert_eq!(code, "/// A price pub fn evil() {} .\n");
}

#[test]
fn a_name_of_another_mod_or_another_format_is_refused() {
    let other = SCHEMA.replace("you.lockers:price", "you.lanterns:price");
    let schema = Schema::parse(&other).unwrap();
    assert!(matches!(
        rust_deps(&schema, "lockers"),
        Err(Error::Invalid {
            what: "name of its owner",
            ..
        })
    ));
    let newer = SCHEMA.replace("schema = 3", "schema = 4\nnew_key = 1");
    let error = Schema::parse(&newer).unwrap_err();
    assert!(matches!(error, Error::Format(4)), "{error}");
    assert!(
        error.to_string().ends_with("update cargo-enshwick"),
        "{error}"
    );
    let older = Schema::parse(&SCHEMA.replace("schema = 3", "schema = 1"))
        .unwrap_err()
        .to_string();
    assert!(
        older.ends_with("rebuild its mod with this enshwick"),
        "{older}"
    );
}
