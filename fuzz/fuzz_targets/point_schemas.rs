//! Another mod's point schemas, from its DLL or as text, written as the
//! definitions an extending mod compiles: whatever the schema holds, the
//! definition is Rust that declares types, named values and layout checks,
//! with docs of plain prose that open no code block rustdoc would run.

#![no_main]
#![expect(
    clippy::panic,
    reason = "a fuzz target reports what it finds by panicking"
)]

use cargo_enshwick::points::{Schema, from_dll, rust};
use libfuzzer_sys::fuzz_target;

/// The attributes a definition carries.
const ATTRIBUTES: [&str; 5] = ["doc", "repr", "derive", "shape", "point"];

/// Whether `c` hides or reorders text, as the writer drops it.
fn is_hidden(c: char) -> bool {
    c.is_control()
        || matches!(
            c,
            '\u{ad}'
                | '\u{61c}'
                | '\u{180e}'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2060}'..='\u{2064}'
                | '\u{2066}'..='\u{2069}'
                | '\u{feff}'
        )
}

fn check_attributes(attrs: &[syn::Attribute]) {
    for attr in attrs {
        let last = attr.path().segments.last().map(|s| s.ident.to_string());
        assert!(
            last.as_deref()
                .is_some_and(|name| ATTRIBUTES.contains(&name)),
            "an attribute a definition does not carry"
        );
        if let syn::Meta::NameValue(pair) = &attr.meta
            && let syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(text),
                ..
            }) = &pair.value
        {
            let text = text.value();
            let body = text.strip_prefix(' ').unwrap_or(&text);
            assert!(!text.chars().any(is_hidden), "a hidden character in a doc");
            assert!(
                !body.starts_with(' ') && !body.starts_with("```") && !body.starts_with("~~~"),
                "a doc line that may open a code block"
            );
        }
    }
}

fn check_item(item: &syn::Item) {
    match item {
        syn::Item::Struct(s) => {
            check_attributes(&s.attrs);
            for field in &s.fields {
                check_attributes(&field.attrs);
            }
        }
        syn::Item::Enum(e) => {
            check_attributes(&e.attrs);
            for variant in &e.variants {
                check_attributes(&variant.attrs);
            }
        }
        syn::Item::Impl(block) => {
            assert!(block.trait_.is_none(), "a definition implements no trait");
            for inner in &block.items {
                let syn::ImplItem::Const(constant) = inner else {
                    panic!("a type's block holds only its named values");
                };
                check_attributes(&constant.attrs);
            }
        }
        syn::Item::Const(constant) => {
            assert_eq!(
                constant.ident, "_",
                "the only free const is the layout check"
            );
            let syn::Expr::Block(block) = &*constant.expr else {
                panic!("the layout check is a block");
            };
            for statement in &block.block.stmts {
                let syn::Stmt::Macro(call) = statement else {
                    panic!("the layout check holds only assertions");
                };
                assert!(
                    call.mac.path.is_ident("assert"),
                    "the layout check holds only assertions"
                );
            }
        }
        _ => panic!("a definition declares only types and checks"),
    }
}

fn written(text: &str) {
    let Ok(schema) = Schema::parse(text) else {
        return;
    };
    let names: Vec<String> = schema.points().map(str::to_owned).collect();
    for name in names {
        let Ok(code) = rust(&schema, &name) else {
            continue;
        };
        let parsed = syn::parse_file(&code);
        assert!(parsed.is_ok(), "a definition is Rust:\n{code}");
        let Ok(file) = parsed else {
            continue;
        };
        check_attributes(&file.attrs);
        file.items.iter().for_each(check_item);
    }
}

fuzz_target!(|data: &[u8]| {
    if let Ok(text) = core::str::from_utf8(data) {
        written(text);
    }
    if let Ok(Some(text)) = from_dll(data) {
        written(&text);
    }
});
