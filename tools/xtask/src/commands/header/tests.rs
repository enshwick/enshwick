use super::*;

#[test]
fn only_annotation_lines_have_their_brackets_unescaped() {
    let source = "/// cbindgen:field-names=\\[bytes\\]\n/// Not \\[here\\].\nlet a = [1];\n";
    assert_eq!(
        cbindgen_annotations(source),
        "/// cbindgen:field-names=[bytes]\n/// Not \\[here\\].\nlet a = [1];\n"
    );
}

#[test]
fn an_optional_callback_is_written_as_the_nullable_callback_type() {
    let cbindgen = "typedef struct EnshwickOption_ActionFn EnshwickOption_ActionFn;\n\
                    \n\
                    typedef void (*EnshwickActionFn)(void);\n\
                    EnshwickStatus (*actions)(struct EnshwickOption_ActionFn callback);\n";
    assert_eq!(
        nullable_callbacks(&format!("{cbindgen}{TRAILER}\n")).replace(&format!("{TRAILER}\n"), ""),
        "typedef void (*EnshwickActionFn)(void);\n\
         EnshwickStatus (*actions)(EnshwickActionFn callback);\n"
    );
}

const GOLDEN: &str = "abi 5.2\nPair size 8 align 4 born 8\n    left at 0 size 4 type u32\n    right at 4 size 4 type extern \"system\" fn(u32)\nStatus size 4 align 4\n    0 at 0 size 4 type i32\n";

#[test]
fn a_struct_and_callback_in_a_domain_folder_reach_the_header_with_their_asserts() {
    let root = Scratch::new("header-test-nested").unwrap();
    let files = [
        (ABI_SOURCE, "mod events;\npub use events::*;\n"),
        (VALUES_ABI_SOURCE, "mod events;\npub use events::*;\n"),
        (
            "crates/internal/values/src/abi/events.rs",
            "/// A value's part.\n#[repr(C)]\npub struct Amount {\n    pub count: u32,\n}\n",
        ),
        (
            "crates/internal/abi/src/abi/events.rs",
            "mod knowledge;\npub use knowledge::*;\n",
        ),
        (
            "crates/internal/abi/src/abi/events/knowledge.rs",
            "/// A deep one.\n#[repr(C)]\npub struct Deep {\n    pub value: u32,\n}\n\
             /// Runs on the game thread.\npub type DeepFn = extern \"system\" fn(value: u32);\n",
        ),
        (
            LAYOUT,
            "abi 1.0\nAmount size 4 align 4\n    count at 0 size 4 type u32\n\
             Deep size 4 align 4\n    value at 0 size 4 type u32\n",
        ),
        (
            CONFIG,
            "language = \"C\"\ntrailer = \"#pragma pack(pop)\"\n",
        ),
    ];
    write_tree(&root, &files);
    let header = generate(&root);
    drop(root);
    let header = header.unwrap();
    assert!(header.contains("typedef struct EnshwickDeep {"), "{header}");
    assert!(
        header.contains("typedef struct EnshwickAmount {"),
        "{header}"
    );
    assert!(
        header.contains("(*EnshwickDeepFn)(uint32_t value)"),
        "{header}"
    );
    assert!(
        header.contains("offsetof(EnshwickDeep, value) == 0"),
        "{header}"
    );
}

/// Writes each `(path, text)` of `files` under `root`, making its folders.
fn write_tree(root: &Path, files: &[(&str, &str)]) {
    for (path, text) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }
}

#[test]
fn callbacks_come_from_declared_and_inline_modules_but_not_from_tests_or_strays() {
    let root = Scratch::new("header-test-callbacks").unwrap();
    write_tree(
        &root,
        &[
            (ABI_SOURCE, "mod events;\n"),
            (
                "crates/internal/abi/src/abi/events.rs",
                "mod knowledge;\n#[cfg(test)]\nmod tests;\n",
            ),
            (
                "crates/internal/abi/src/abi/events/knowledge.rs",
                "pub type DeepFn = extern \"system\" fn();\n\
                 pub mod hooks {\n    pub type ProbeFn = extern \"system\" fn(u32);\n}\n\
                 #[cfg(test)]\nmod tests {\n    pub type InlineFixtureFn = extern \"system\" fn();\n}\n",
            ),
            (
                "crates/internal/abi/src/abi/events/tests.rs",
                "pub type FixtureFn = extern \"system\" fn();\n",
            ),
            (
                "crates/internal/abi/src/abi/stray.rs",
                "pub type StrayFn = extern \"system\" fn();\n",
            ),
        ],
    );
    assert_eq!(callback_types(&root).unwrap(), ["DeepFn", "ProbeFn"]);
}

#[test]
fn a_failed_copy_names_the_scratch_path_it_could_not_write() {
    let root = Scratch::new("header-test-copy").unwrap();
    write_tree(&root, &[(ABI_SOURCE, ""), (VALUES_ABI_SOURCE, "")]);
    let taken = root.join("taken");
    std::fs::write(&taken, "").unwrap();
    let error = copy_abi(&root, &taken).unwrap_err();
    let into = taken.join(ABI_COPY).display().to_string();
    assert!(error.starts_with(&into), "{error}");
}

#[test]
fn the_layout_file_reads_into_shapes() {
    let shapes = parse_layout(GOLDEN).unwrap();
    assert_eq!(shapes.len(), 2);
    assert_eq!(shapes[0].name, "Pair");
    assert_eq!(
        shapes[0].fields,
        [("left".to_owned(), 0), ("right".to_owned(), 4)]
    );
    assert_eq!((shapes[1].size, shapes[1].align), (4, 4));
}

#[test]
fn the_golden_layout_gives_the_host_table_fields_in_order() {
    let layout = "abi 5.0\nHeader size 8 align 4\n    size at 0 size 4 type u32\n\
                  HostApi size 16 align 8\n    header at 0 size 8 type Header\n    log at 8 size 8 type fn\n";
    assert_eq!(
        host_api_fields(layout),
        Ok(vec!["header".to_owned(), "log".to_owned()])
    );
    assert_eq!(
        host_api_fields(GOLDEN),
        Err(format!("{LAYOUT}: no HostApi"))
    );
}

#[test]
fn a_malformed_layout_line_is_named() {
    let error =
        parse_layout("abi 5.0\nPair size 8 align 4\nPair size eight align 4\n").unwrap_err();
    assert!(error.contains(":3:"), "{error}");
}

#[test]
fn asserts_cover_sizes_alignments_and_named_fields_only() {
    let header = "typedef struct EnshwickPair {\n  // The left.\n  int left;\n  int right;\n} EnshwickPair;\n";
    let c = asserts(&parse_layout(GOLDEN).unwrap(), header);
    assert!(c.contains("sizeof(EnshwickPair) == 8"));
    assert!(c.contains("ENSHWICK_ALIGNOF(EnshwickPair) == 4"));
    assert!(c.contains("offsetof(EnshwickPair, right) == 4"));
    assert!(c.contains("sizeof(EnshwickStatus) == 4"));
    assert!(!c.contains("offsetof(EnshwickStatus"));
}

#[test]
fn a_member_cbindgen_escaped_is_named_as_escaped() {
    let body = vec![
        "  uint32_t template_;",
        "  uint32_t newer;",
        "  Status (*new_)(int a);",
    ];
    assert_eq!(c_member(&body, "template"), "template_");
    assert_eq!(c_member(&body, "new"), "new_");
    assert_eq!(c_member(&body, "newer"), "newer");
}

#[test]
fn a_header_cpp_refuses_fails_only_the_cpp_compile() {
    let clang = Command::new("clang")
        .env("PATH", crate::tool_path())
        .arg("--version")
        .output();
    if clang.is_err() {
        return;
    }
    let include = "#include \"enshwick.h\"\n";
    let dir = Scratch::new("header-test-header").unwrap();
    let header = dir.join("enshwick.h");
    std::fs::write(&header, "void enshwick_f(int template);\n").unwrap();
    let c = compile(&header, Language::C, Packing::Default, include);
    let cpp = compile(&header, Language::Cpp, Packing::Default, include);
    std::fs::write(&header, "void enshwick_f(int defaults);\n").unwrap();
    let renamed = compile(&header, Language::Cpp, Packing::Default, include);
    drop(dir);
    assert_eq!(c, Vec::<String>::new());
    assert!(
        !cpp.is_empty() && cpp.iter().all(|line| line.contains("in C++")),
        "{cpp:?}"
    );
    assert_eq!(renamed, Vec::<String>::new());
}

#[test]
fn the_header_s_packing_holds_under_one_byte_packing_and_its_asserts_catch_a_change() {
    let clang = Command::new("clang")
        .env("PATH", crate::tool_path())
        .arg("--version")
        .output();
    if clang.is_err() {
        return;
    }
    let layout = "abi 5.0\nPair size 16 align 8\n    tag at 0 size 1 type u8\n    value at 8 size 8 type u64\n";
    let body = "#include <stddef.h>\n#include <stdint.h>\n\
                #define ENSHWICK_STATIC_ASSERT(c, m) _Static_assert(c, m)\n\
                #define ENSHWICK_ALIGNOF(t) _Alignof(t)\n\
                typedef struct EnshwickPair {\n  uint8_t tag;\n  uint64_t value;\n} EnshwickPair;\n";
    let asserts = asserts(&parse_layout(layout).unwrap(), body);
    let dir = Scratch::new("header-test-packing").unwrap();
    let header = dir.join("enshwick.h");
    let compiled = |text: String| {
        std::fs::write(&header, text).unwrap();
        compile(&header, Language::C, Packing::One, INCLUDE)
    };
    let packed = compiled(format!("#pragma pack(push, 8)\n{body}{asserts}{TRAILER}\n"));
    let unpacked = compiled(format!("{body}{asserts}"));
    drop(dir);
    assert_eq!(packed, Vec::<String>::new());
    assert!(
        unpacked.iter().any(|line| line.contains("size of Pair")),
        "{unpacked:?}"
    );
}

#[test]
fn rust_paths_in_comment_code_spans_become_c_names() {
    let types = [
        "Status".to_owned(),
        "HostApi".to_owned(),
        "HookSpec".to_owned(),
        "HostApi::log".to_owned(),
    ];
    let header = "// `Status::OK`, `HostApi::log`, `HookSpec`, `u32::MAX`, `usize::MAX`.\n\
                  // `size_of::<HookSpec>()` and `Status::OK | Status::REFUSED`, `Other::X`, `HookSpec::new`.\n\
                  int Status::OK;\n";
    assert_eq!(
        c_paths(header, &types),
        "// `EnshwickStatus_OK`, `EnshwickHostApi.log`, `EnshwickHookSpec`, `UINT32_MAX`, `SIZE_MAX`.\n\
         // `sizeof(EnshwickHookSpec)` and `EnshwickStatus_OK | EnshwickStatus_REFUSED`, `Other::X`, `HookSpec::new`.\n\
         int Status::OK;\n"
    );
}

#[test]
fn a_comment_left_with_a_rust_path_or_macro_is_named() {
    let header = "// `EnshwickStatus_OK`\n// `Other::X`\nint a::b;\n// `declare_mod!`\n";
    let problems = rust_in_comments(header);
    assert_eq!(problems.len(), 2, "{problems:?}");
    assert!(problems[0].contains(":2:"), "{problems:?}");
    assert!(problems[1].contains(":4:"), "{problems:?}");
}

#[test]
fn a_callback_whose_comment_names_no_thread_is_named() {
    let header = "// Runs on the game thread.\ntypedef void (*EnshwickGoodFn)(void);\n\
                  \n// Answers 1.\ntypedef int (*EnshwickBadFn)(void);\n\
                  typedef struct EnshwickPair { int a; } EnshwickPair;\n";
    let problems = callbacks_without_thread(header);
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("EnshwickBadFn"), "{problems:?}");
}

#[test]
fn the_str_pointer_is_a_char_pointer_in_cpp_only() {
    let header =
        "typedef struct EnshwickStr {\n  const uint8_t *ptr;\n  size_t len;\n} EnshwickStr;\n";
    assert_eq!(
        cpp_str_pointer(header),
        "typedef struct EnshwickStr {\n#ifdef __cplusplus\n  const char *ptr;\n#else\n  const uint8_t *ptr;\n#endif\n  size_t len;\n} EnshwickStr;\n"
    );
}

#[test]
fn a_failed_compile_that_names_no_error_still_fails() {
    let lines = failures("C++", "exit code: 1", "warning: only a warning\n");
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("named no error"), "{lines:?}");
    assert_eq!(
        failures("C", "exit code: 1", "a.c:1:1: error: bad\nnote: here\n"),
        vec![format!(
            "{HEADER} against {LAYOUT} in C: a.c:1:1: error: bad"
        )]
    );
}

#[test]
fn rustdoc_links_become_plain_code() {
    assert_eq!(
        plain_links("// the [`Header`] of [`HostApi::mods`](super::HostApi::mods)."),
        "// the `Header` of `HostApi::mods`."
    );
    assert_eq!(plain_links("// a [`dangling"), "// a [`dangling");
    assert_eq!(
        plain_links("// see [actions](HostApi::actions)."),
        "// see actions."
    );
    assert_eq!(
        plain_links("// levels[0] and [x] stay"),
        "// levels[0] and [x] stay"
    );
}

#[test]
fn examples_sections_are_left_out_of_comments() {
    let header =
        "// A header.\n//\n// # Examples\n//\n// ```\n// rust();\n// ```\ntypedef int A;\n";
    assert_eq!(tidy(header), "// A header.\ntypedef int A;\n");
}

#[test]
fn capital_constants_take_the_c_prefix_everywhere() {
    let header = "#define EnshwickMAX_ZONES 4\n#define EnshwickStatus_OK 0\nint zones[EnshwickMAX_ZONES];\nint EnshwickMAX_ZONES_LEFT;\n";
    assert_eq!(
        tidy(header),
        "#define ENSHWICK_MAX_ZONES 4\n#define EnshwickStatus_OK 0\nint zones[ENSHWICK_MAX_ZONES];\nint EnshwickMAX_ZONES_LEFT;\n"
    );
}
