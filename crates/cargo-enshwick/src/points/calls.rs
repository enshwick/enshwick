//! Another mod's calls, as its schema describes them.

use serde::Deserialize;

/// One call.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(super) struct CallSchema {
    pub(super) name: String,
    pub(super) input: String,
    pub(super) doc: String,
    pub(super) major: u32,
    pub(super) output: String,
    pub(super) output_layout: String,
    pub(super) input_size: u32,
    pub(super) output_size: u32,
    pub(super) layout: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::super::{Error, Schema, luau, rust_deps};

    const SCHEMA: &str = r#"schema = 1
mod = "you.trading-post"
version = "1.2.0"

[[call]]
name = "you.trading-post:price-of"
input = "PriceOf"
doc = "What the trading post pays for an item."
major = 2
output = "u32"
output_layout = "17"
input_size = 8
output_size = 4
layout = ["1", "2"]

[[type]]
name = "PriceOf"
doc = "What the trading post pays for an item."
kind = "struct"

[[type.field]]
name = "item"
doc = "The item."
type = "u32"

[[type.field]]
name = "deal"
doc = ""
type = "Deal"

[[type]]
name = "Deal"
doc = "How the item changes hands."
kind = "enum"
repr = "u8"

[[type.variant]]
name = "Buy"
value = 0

[[type.variant]]
name = "Sell"
value = 1
"#;
    #[test]
    fn a_call_becomes_a_declaration_of_the_alias_with_its_types_and_checks() {
        let schema = Schema::parse(SCHEMA).unwrap();
        assert_eq!(
            schema.calls().collect::<Vec<_>>(),
            ["you.trading-post:price-of"]
        );
        let code = rust_deps(&schema, "trading_post").unwrap();
        insta::assert_snapshot!(code);
    }

    #[test]
    fn the_declarations_of_the_fixture_are_the_ones_the_sdk_compiles() {
        let text = include_str!("../../tests/fixtures/lockers.calls.toml");
        let schema = Schema::parse(text).unwrap();
        let code = rust_deps(&schema, "lockers").unwrap();
        assert_eq!(code, include_str!("../../tests/fixtures/lockers_calls.rs"));
    }

    #[test]
    fn the_luau_module_of_the_fixture_is_the_one_the_runtime_is_tested_with() {
        let text = include_str!("../../tests/fixtures/lockers.calls.toml");
        let schema = Schema::parse(text).unwrap();
        let code = luau::luau_call(&schema, "you.lockers:fee", "lockers").unwrap();
        assert_eq!(code, include_str!("../../tests/fixtures/lockers_fee.luau"));
    }

    #[test]
    fn a_luau_module_refuses_a_value_type_or_enum_in_the_input() {
        let text = include_str!("../../tests/fixtures/lockers.calls.toml");
        let schema = Schema::parse(text).unwrap();
        let error = luau::luau_call(&schema, "you.lockers:rent", "lockers").unwrap_err();
        assert!(
            matches!(&error, Error::ScriptInput { field, ty, .. } if field == "deal" && ty == "Deal"),
            "{error}"
        );
    }

    #[test]
    fn the_luau_modules_leave_out_a_call_no_script_can_fill_and_say_why() {
        let text = include_str!("../../tests/fixtures/lockers.calls.toml");
        let schema = Schema::parse(text).unwrap();
        let deps = super::super::luau_deps(&schema, "lockers").unwrap();
        let names: Vec<&str> = deps.modules.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["fee"]);
        assert_eq!(deps.skipped.len(), 1);
        assert!(
            deps.skipped[0].to_string().contains("you.lockers:rent"),
            "{}",
            deps.skipped[0]
        );
    }

    #[test]
    fn a_bad_call_name_is_refused() {
        let text = SCHEMA.replace("you.trading-post:price-of", "You.Trading:price-of");
        assert!(Schema::parse(&text).is_err());
    }
}
