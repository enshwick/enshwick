use super::*;

const SOURCE: &str = "\
use core::time::Duration;

use super::{Load, Off, ok};

/// Not moved.
pub struct Kept;

#[crate::script(ticks)]
impl Load<'_> {
    /// Calls `handler` every `every`, for the mod's clock — «тік».
    ///
    /// # Examples
    ///
    /// ```
    /// let a = 1;
    /// ```
    #[script]
    #[must_use]
    pub fn on_tick<E: Copy, const N: usize>(
        &mut self,
        every: Duration,
    ) -> Result<(), Off>
    where
        E: Send,
    {
        ok(every)
    }

    pub(crate) fn inner(&self) -> u32 {
        1
    }
}

impl Load<'_> {
    fn hidden(&self) {}
}
";

#[test]
fn a_public_method_moves_with_one_doc_line_and_leaves_a_forwarder_with_its_docs()
-> Result<(), String> {
    let peeled = peel(SOURCE, &Form::Base)?;
    assert_eq!(
        peeled.base,
        "\
use core::time::Duration;

use super::{Load, Off, ok};

/// Not moved.
pub struct Kept;

impl BaseLoad {
    /// As `Load::on_tick`.
    #[doc(hidden)]
    #[must_use]
    pub fn on_tick<E: Copy, const N: usize>(
        &mut self,
        every: Duration,
    ) -> Result<(), Off>
    where
        E: Send,
    {
        ok(every)
    }

    pub(crate) fn inner(&self) -> u32 {
        1
    }
}

impl BaseLoad {
    fn hidden(&self) {}
}
"
    );
    assert_eq!(
        peeled.forwarders,
        ["\
#[enshwick_macros::forward(base)]
#[crate::script(ticks)]
impl Load<'_> {
    /// Calls `handler` every `every`, for the mod's clock — «тік».
    ///
    /// # Examples
    ///
    /// ```
    /// let a = 1;
    /// ```
    #[script]
    #[must_use]
    pub fn on_tick<E: Copy, const N: usize>(
        &mut self,
        every: Duration,
    ) -> Result<(), Off>
    where
        E: Send;
}"]
    );
    assert_eq!(peeled.uses, ["use core::time::Duration;"]);
    Ok(())
}

#[test]
fn peel_load_writes_each_module_and_starts_or_extends_its_forwarder_file() {
    let root = std::env::temp_dir().join(format!("xtask-peel-{}", std::process::id()));
    std::fs::create_dir_all(root.join("base")).unwrap();
    let module = "use crate::Host;\n\nimpl Load<'_> {\n    /// Docs.\n    pub fn f(&self) {}\n}\n";
    std::fs::write(root.join("base/a.rs"), module).unwrap();
    std::fs::write(root.join("base/b.rs"), module.replace("fn f", "fn g")).unwrap();
    std::fs::write(
        root.join("base/c.rs"),
        "impl Load<'_> {\n    fn h(&self) {}\n}\n",
    )
    .unwrap();
    let ran = run(
        &root,
        &[
            "base/a.rs=core/load/a.rs".to_owned(),
            "base/b.rs=core/load/a.rs".to_owned(),
            "base/c.rs=core/load/c.rs".to_owned(),
        ],
    );
    let unpaired = run(&root, &["base/a.rs".to_owned()]);
    std::fs::create_dir_all(root.join("core/load")).unwrap();
    std::fs::write(root.join("core/load/odd.rs"), [0xff, 0xfe]).unwrap();
    std::fs::write(root.join("base/d.rs"), module).unwrap();
    let unreadable = run(&root, &["base/d.rs=core/load/odd.rs".to_owned()]);
    let forwarders = std::fs::read_to_string(root.join("core/load/a.rs")).unwrap();
    let peeled = std::fs::read_to_string(root.join("base/a.rs")).unwrap();
    let private_only = root.join("core/load/c.rs").exists();
    let odd = std::fs::read(root.join("core/load/odd.rs")).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(ran, Ok(true));
    assert_eq!(unpaired, Err("base/a.rs: not <from>=<into>".to_owned()));
    assert!(
        unreadable.as_ref().is_err_and(|e| e.contains("odd.rs")),
        "a forwarder file that does not read is no new file: {unreadable:?}"
    );
    assert_eq!(odd, [0xff, 0xfe], "and stays as it was");
    assert!(peeled.contains("impl BaseLoad {"), "{peeled}");
    assert!(
        forwarders.starts_with("//! `Load`'s methods")
            && forwarders.contains("use crate::Host;\nuse crate::Load;\nuse crate::Off;\n")
            && forwarders.contains("pub fn f(&self);")
            && forwarders.ends_with("    /// Docs.\n    pub fn g(&self);\n}\n"),
        "{forwarders}"
    );
    assert_eq!(forwarders.matches("//! `Load`'s").count(), 1, "one header");
    assert!(
        !private_only,
        "a block without a public method forwards nothing"
    );
}

#[test]
fn a_new_forwarder_file_names_load_and_its_source_s_imports() {
    let text = header(&["use core::time::Duration;".to_owned()], None);
    assert!(text.ends_with("use core::time::Duration;\nuse crate::Load;\nuse crate::Off;\n"));
    let text = header(&["use crate::{Host, Off};".to_owned()], None);
    assert!(text.ends_with("use crate::{Host, Off};\nuse crate::Load;\n"));
}

#[test]
fn other_impls_stay_and_a_load_block_keeps_its_consts_and_types() -> Result<(), String> {
    let source = "impl Other {\n    pub fn f(&self) {}\n}\n\nimpl Clone for Load<'_> {\n    fn clone(&self) -> Self {\n        todo!()\n    }\n}\n\nimpl Load<'_> {\n    /// A part.\n    pub const PART: u32 = 1;\n\n    type Kind = u8;\n\n    fn inner(&self) {}\n}\n";
    let peeled = peel(source, &Form::Base)?;
    assert!(peeled.forwarders.is_empty());
    assert!(
        peeled
            .base
            .starts_with("impl Other {\n    pub fn f(&self) {}\n}\n\nimpl Clone for Load<'_> {")
    );
    assert!(peeled.base.ends_with(
            "impl BaseLoad {\n    /// A part.\n    pub const PART: u32 = 1;\n\n    type Kind = u8;\n\n    fn inner(&self) {}\n}\n"
        ));
    Ok(())
}

#[test]
fn a_forwarder_file_takes_no_re_export_nor_an_import_of_the_module_s_own_children()
-> Result<(), String> {
    let source = "mod script;\n\n#[doc(hidden)]\npub use script::Fns;\nuse script::Inner;\nuse self::x::Y;\nuse super::Z;\n#[cfg(test)]\nuse crate::Host;\nuse serde::Deserialize as _;\nuse toml;\nuse bytemuck as bm;\nuse script;\n";
    assert_eq!(
        peel(source, &Form::Base)?.uses,
        [
            "#[cfg(test)]\nuse crate::Host;",
            "use serde::Deserialize as _;",
            "use toml;",
            "use bytemuck as bm;"
        ]
    );
    Ok(())
}

#[test]
fn a_moved_unsafe_method_keeps_its_safety_section() -> Result<(), String> {
    let source = "impl Load<'_> {\n    /// Hooks.\n    ///\n    /// # Safety\n    ///\n    /// - `at` starts a function.\n    ///\n    /// # Errors\n    ///\n    /// When refused.\n    pub unsafe fn hook(&mut self, at: u64) {}\n}\n";
    assert_eq!(
        peel(source, &Form::Base)?.base,
        "impl BaseLoad {\n    /// As `Load::hook`.\n    ///\n    /// # Safety\n    ///\n    /// - `at` starts a function.\n    #[doc(hidden)]\n    pub unsafe fn hook(&mut self, at: u64) {}\n}\n"
    );
    Ok(())
}

#[test]
fn a_macro_call_in_a_load_block_is_refused_rather_than_dropped() {
    let source =
        "impl Other {\n    m!();\n}\n\nimpl Load<'_> {\n    pub fn f(&self) {}\n\n    m!();\n}\n";
    assert_eq!(
            peel(source, &Form::Base),
            Err("line 5: an `impl Load` block with an item other than a method, constant or type; move it by hand".to_owned())
        );
}

#[test]
fn a_script_adapter_moves_public_and_keeps_its_visibility_on_load() -> Result<(), String> {
    let source = "#[cfg(feature = \"unstable\")]\n#[crate::script(texts)]\nimpl Load<'_> {\n    /// Texts for a script.\n    #[script(name = \"texts\")]\n    pub(crate) fn texts_script(&mut self) -> u32 {\n        1\n    }\n}\n";
    let peeled = peel(source, &Form::Base)?;
    assert_eq!(
        peeled.base,
        "#[cfg(feature = \"unstable\")]\nimpl BaseLoad {\n    /// As `Load::texts_script`.\n    #[doc(hidden)]\n    pub fn texts_script(&mut self) -> u32 {\n        1\n    }\n}\n"
    );
    assert_eq!(
        peeled.forwarders,
        [
            "#[cfg(feature = \"unstable\")]\n#[enshwick_macros::forward(base)]\n#[crate::script(texts)]\nimpl Load<'_> {\n    /// Texts for a script.\n    #[script(name = \"texts\")]\n    pub(crate) fn texts_script(&mut self) -> u32;\n}"
        ]
    );
    Ok(())
}

#[test]
fn a_module_above_base_gets_one_method_trait_for_all_its_blocks() -> Result<(), String> {
    let source = "use crate::Off;\n\n#[cfg(feature = \"unstable\")]\n#[crate::script(items)]\nimpl Load<'_> {\n    /// Adds an item.\n    #[must_use]\n    #[expect(clippy::x, reason = \"y\")]\n    pub fn add(&mut self, mut id: u32) -> Result<(), Off> {\n        self.helper(id)\n    }\n\n    pub(crate) fn helper(&mut self, id: u32) -> Result<(), Off> {\n        Ok(())\n    }\n}\n\nimpl Load<'_> {\n    fn quiet(&self) {}\n}\n";
    let form = Form::of("crates/internal/content/src/content/changes/items.rs");
    assert_eq!(
        form,
        Form::Trait {
            krate: "enshwick_content".to_owned(),
            module: "content::changes::items".to_owned(),
            reexports: "crates/internal/content/src/__load.rs".to_owned(),
        }
    );
    let peeled = peel(source, &form)?;
    assert_eq!(
        peeled.base,
        "use crate::Off;\n\n/// `Load`'s methods of this module on the base handle, which core's\n/// forwarders call.\n#[doc(hidden)]\npub trait LoadMethods {\n    /// As `Load::add`.\n    #[doc(hidden)]\n    #[cfg(feature = \"unstable\")]\n    #[must_use]\n    fn add(&mut self, id: u32) -> Result<(), Off>;\n}\n\nimpl LoadMethods for crate::BaseLoad {\n    #[cfg(feature = \"unstable\")]\n    #[expect(clippy::x, reason = \"y\")]\n    fn add(&mut self, mut id: u32) -> Result<(), Off> {\n        self.helper(id)\n    }\n}\n\n/// The base handle's helpers of this module for the crate's own code.\npub(crate) trait LoadHelpers {\n    #[cfg(feature = \"unstable\")]\n    fn helper(&mut self, id: u32) -> Result<(), Off>;\n\n    fn quiet(&self);\n}\n\nimpl LoadHelpers for crate::BaseLoad {\n    #[cfg(feature = \"unstable\")]\n    fn helper(&mut self, id: u32) -> Result<(), Off> {\n        Ok(())\n    }\n\n    fn quiet(&self) {}\n}\n\n\n"
    );
    assert_eq!(
        peeled.forwarders,
        [
            "#[cfg(feature = \"unstable\")]\n#[enshwick_macros::forward(base)]\n#[crate::script(items)]\nimpl Load<'_> {\n    /// Adds an item.\n    #[must_use]\n    #[expect(clippy::x, reason = \"y\")]\n    pub fn add(&mut self, mut id: u32) -> Result<(), Off>;\n}"
        ]
    );
    assert_eq!(peeled.traits, ["LoadMethods", "LoadHelpers"]);
    assert_eq!(
        header(&[], peeled.krate.as_deref()),
        "//! `Load`'s methods of a module below core, forwarded to the base handle.\n\nuse enshwick_content::__load::*;\nuse crate::Load;\nuse crate::Off;\n"
    );
    assert_eq!(Form::of("crates/internal/base/src/load/a.rs"), Form::Base);
    assert!(
        peel("impl Load<'_> {\n    const A: u8 = 1;\n}\n", &form)
            .is_err_and(|e| e.contains("move it by hand"))
    );
    Ok(())
}

#[test]
fn a_forwarder_file_gains_the_trait_glob_once_and_the_crate_re_exports_each_trait_once() {
    let kept = "//! Doc.\n\nuse crate::Load;\n".to_owned();
    let globbed = with_glob(kept.clone(), Some("enshwick_content"));
    assert_eq!(
        globbed,
        "//! Doc.\n\nuse enshwick_content::__load::*;\nuse crate::Load;\n"
    );
    assert_eq!(
        with_glob(globbed.clone(), Some("enshwick_content")),
        globbed
    );
    assert_eq!(with_glob(kept.clone(), None), kept);
    let file = std::env::temp_dir().join(format!("xtask-reexport-{}.rs", std::process::id()));
    let first = reexport(&file, "content::items", "LoadMethods");
    let again = reexport(&file, "content::items", "LoadMethods");
    let second = reexport(&file, "rates", "LoadHelpers");
    let text = std::fs::read_to_string(&file).unwrap();
    std::fs::remove_file(&file).unwrap();
    assert_eq!((first, again, second), (Ok(()), Ok(()), Ok(())));
    assert!(text.starts_with("//! The traits"), "{text}");
    assert!(
            text.ends_with(
                "\n\npub use crate::content::items::LoadMethods as _;\npub(crate) use crate::rates::LoadHelpers as _;\n"
            ),
            "{text}"
        );
}
