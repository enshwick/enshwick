//! Another mod's point schemas, from its DLL or as text, written as the
//! definitions an extending mod compiles: whatever the schema holds, the
//! definition is Rust that declares types and layout checks, and nothing
//! else.

#![no_main]

use cargo_enshwick::points::{Schema, from_dll, rust};
use libfuzzer_sys::fuzz_target;

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
        for item in &file.items {
            assert!(
                matches!(
                    item,
                    syn::Item::Struct(_)
                        | syn::Item::Enum(_)
                        | syn::Item::Impl(_)
                        | syn::Item::Const(_)
                ),
                "a definition declares only types and checks"
            );
            if let syn::Item::Impl(block) = item {
                assert!(
                    block
                        .items
                        .iter()
                        .all(|i| matches!(i, syn::ImplItem::Const(_))),
                    "a type's block holds only its named values"
                );
            }
        }
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
