//! `cargo xtask peel-load <from>=<into>...`: the step of a peel that moves
//! `Load`'s methods off `enshwick-core`. Each `impl Load` block of the
//! module file `from`, already moved into the crate below, becomes an
//! `impl BaseLoad` block whose public methods carry one doc line; their
//! docs, examples and `#[script]` go to a forwarder block on `Load`
//! appended to `into` in core, whose bodies `#[forward(base)]` writes.

use std::fmt::Write as _;
use std::path::Path;

use proc_macro2::{LineColumn, Span};
use syn::{Attribute, ImplItem, ImplItemFn, Item, ItemImpl, Type, Visibility};

/// The base handle's type, which the moved blocks implement.
const BASE: &str = "BaseLoad";
/// The field of `Load` that holds the base handle.
const FIELD: &str = "base";

/// Peels each `from=into` pair of `args`, paths from the workspace root.
///
/// # Errors
///
/// When an argument is no pair, or a file does not read, parse or write.
pub(crate) fn run(root: &Path, args: &[String]) -> Result<bool, String> {
    if args.is_empty() {
        return Err("usage: cargo xtask peel-load <from>=<into>...".to_owned());
    }
    for arg in args {
        let (from, into) = arg
            .split_once('=')
            .ok_or_else(|| format!("{arg}: not <from>=<into>"))?;
        let (from, into) = (root.join(from), root.join(into));
        let text =
            std::fs::read_to_string(&from).map_err(|e| format!("{}: {e}", from.display()))?;
        let peeled = peel(&text).map_err(|e| format!("{}: {e}", from.display()))?;
        // The forwarders, which keep the docs, are written before `from`
        // loses them.
        if !peeled.forwarders.is_empty() {
            write_forwarders(&into, &peeled)?;
            crate::say(&format!(
                "{}: {} forwarder blocks into {}",
                from.display(),
                peeled.forwarders.len(),
                into.display()
            ));
        }
        std::fs::write(&from, &peeled.base).map_err(|e| format!("{}: {e}", from.display()))?;
    }
    Ok(true)
}

/// Appends the forwarders of `peeled` to `into`, a new file when it is
/// missing.
fn write_forwarders(into: &Path, peeled: &Peeled) -> Result<(), String> {
    {
        let mut out = match std::fs::read_to_string(into) {
            Ok(kept) => kept,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => header(&peeled.uses),
            Err(e) => return Err(format!("{}: {e}", into.display())),
        };
        for block in &peeled.forwarders {
            out.push('\n');
            out.push_str(block);
            out.push('\n');
        }
        if let Some(dir) = into.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        }
        std::fs::write(into, out).map_err(|e| format!("{}: {e}", into.display()))?;
    }
    Ok(())
}

fn header(uses: &[String]) -> String {
    let mut out = String::from(
        "//! `Load`'s methods of a module below core, forwarded to the base handle.\n\n",
    );
    for line in uses {
        out.push_str(line);
        out.push('\n');
    }
    // `Load` and `Off`, each unless a copied import names it already.
    let named = |name: &str| {
        uses.iter().any(|line| {
            line.split(|c: char| !c.is_alphanumeric() && c != '_')
                .any(|word| word == name)
        })
    };
    for name in ["Load", "Off"].into_iter().filter(|name| !named(name)) {
        let _written = writeln!(out, "use crate::{name};");
    }
    out
}

/// A module file after its peel.
#[derive(Debug, PartialEq, Eq)]
struct Peeled {
    /// The file with each `impl Load` block on the base handle.
    base: String,
    /// One `Load` block of forwarders per block with a public method.
    forwarders: Vec<String>,
    /// The file's `use` items, but those of `super`, for a new forwarder file.
    uses: Vec<String>,
}

/// Peels `text`, a module file.
fn peel(text: &str) -> Result<Peeled, String> {
    let file = syn::parse_file(text).map_err(|e| e.to_string())?;
    let at = Offsets::new(text);
    let mut base = text.to_owned();
    let mut forwarders = Vec::new();
    let mut uses = Vec::new();
    let mut cuts = Vec::new();
    for item in &file.items {
        match item {
            Item::Use(item) => {
                let start = at.byte(item.use_token.span.start());
                let end = at.byte(item.semi_token.span.end());
                let line = &text[attrs_start(&item.attrs, &at).unwrap_or(start)..end];
                if !line.contains("super::") {
                    uses.push(line.to_owned());
                }
            }
            Item::Impl(item) if is_load(item) => {
                let start =
                    attrs_start(&item.attrs, &at).unwrap_or(at.byte(item.impl_token.span.start()));
                let end = at.byte(item.brace_token.span.close().end());
                let (moved, forwarder) = block(item, text, &at);
                cuts.push((start, end, moved));
                forwarders.extend(forwarder);
            }
            _ => {}
        }
    }
    for (start, end, moved) in cuts.into_iter().rev() {
        base.replace_range(start..end, &moved);
    }
    Ok(Peeled {
        base,
        forwarders,
        uses,
    })
}

/// Whether `item` is an inherent `impl Load` block.
fn is_load(item: &ItemImpl) -> bool {
    item.trait_.is_none()
        && matches!(&*item.self_ty, Type::Path(path)
            if path.path.segments.last().is_some_and(|s| s.ident == "Load"))
}

/// The base block that replaces `item` and its forwarder block, if it has a
/// public method.
fn block(item: &ItemImpl, text: &str, at: &Offsets<'_>) -> (String, Option<String>) {
    let mut moved = Vec::new();
    let mut bodiless = Vec::new();
    for entry in &item.items {
        let ImplItem::Fn(method) = entry else {
            moved.push(format!("    {}", slice_of(entry, text, at)));
            continue;
        };
        let start = item_start(method, at);
        let sig_end = at.byte(method.block.brace_token.span.open().start());
        let end = at.byte(method.block.brace_token.span.close().end());
        let public = matches!(method.vis, Visibility::Public(_));
        // A script adapter names the scripts' function on `Load`, whatever
        // its visibility.
        let scripted = method.attrs.iter().any(|a| a.path().is_ident("script"));
        if !public && !scripted {
            moved.push(format!("    {}", &text[start..end]));
            continue;
        }
        // Hidden: its docs, `# Errors` and `# Safety` sections are `Load`'s.
        let mut lines = format!(
            "    /// As `Load::{}`.\n    #[doc(hidden)]\n",
            method.sig.ident
        );
        for attr in method
            .attrs
            .iter()
            .filter(|a| !a.path().is_ident("doc") && !a.path().is_ident("script"))
        {
            let _written = writeln!(lines, "    {}", attr_text(attr, text, at));
        }
        lines.push_str("    ");
        let signature_start = sig_start(method, at);
        match &method.vis {
            Visibility::Public(_) => lines.push_str(&text[signature_start..end]),
            Visibility::Restricted(r) => {
                lines.push_str("pub");
                lines.push_str(&text[at.byte(r.paren_token.span.close().end())..end]);
            }
            Visibility::Inherited => {
                lines.push_str("pub ");
                lines.push_str(&text[signature_start..end]);
            }
        }
        moved.push(lines);
        let sig = text[start..sig_end].trim_end();
        bodiless.push(format!("    {};", sig.strip_suffix(',').unwrap_or(sig)));
    }
    // The block's own attributes, such as a `cfg`, hold for both blocks;
    // `#[script]` names `Load`'s functions and goes with the forwarders.
    let is_script = |a: &&Attribute| {
        a.path()
            .segments
            .last()
            .is_some_and(|s| s.ident == "script")
    };
    let mut kept = String::new();
    for attr in item.attrs.iter().filter(|a| !is_script(a)) {
        kept.push_str(attr_text(attr, text, at));
        kept.push('\n');
    }
    let moved = format!("{kept}impl {BASE} {{\n{}\n}}", moved.join("\n\n"));
    if bodiless.is_empty() {
        return (moved, None);
    }
    let mut head = format!("{kept}#[enshwick_macros::forward({FIELD})]\n");
    for attr in item.attrs.iter().filter(is_script) {
        head.push_str(attr_text(attr, text, at));
        head.push('\n');
    }
    let forwarder = format!("{head}impl Load<'_> {{\n{}\n}}", bodiless.join("\n\n"));
    (moved, Some(forwarder))
}

/// The text of an impl item other than a method, from its first attribute.
fn slice_of<'t>(entry: &ImplItem, text: &'t str, at: &Offsets<'_>) -> &'t str {
    let (attrs, first, last): (&[Attribute], Span, Span) = match entry {
        ImplItem::Const(c) => (&c.attrs, c.const_token.span, c.semi_token.span),
        ImplItem::Type(t) => (&t.attrs, t.type_token.span, t.semi_token.span),
        _ => return "",
    };
    let start = attrs_start(attrs, at).unwrap_or(at.byte(first.start()));
    &text[start..at.byte(last.end())]
}

/// Where a method starts: its first attribute, else its signature.
fn item_start(method: &ImplItemFn, at: &Offsets<'_>) -> usize {
    attrs_start(&method.attrs, at).unwrap_or_else(|| sig_start(method, at))
}

/// Where a method's visibility or signature starts.
fn sig_start(method: &ImplItemFn, at: &Offsets<'_>) -> usize {
    let sig = &method.sig;
    let spans = [
        match &method.vis {
            Visibility::Public(token) => Some(token.span),
            Visibility::Restricted(r) => Some(r.pub_token.span),
            Visibility::Inherited => None,
        },
        sig.constness.as_ref().map(|t| t.span),
        sig.asyncness.as_ref().map(|t| t.span),
        match &sig.safety {
            syn::Safety::Unsafe(token) => Some(token.span),
            syn::Safety::Safe(token) => Some(token.span),
            syn::Safety::Default => None,
        },
        sig.abi.as_ref().map(|a| a.extern_token.span),
        Some(sig.fn_token.span),
    ];
    spans
        .into_iter()
        .flatten()
        .map(|span| at.byte(span.start()))
        .min()
        .unwrap_or_default()
}

/// Where the first of `attrs` starts.
fn attrs_start(attrs: &[Attribute], at: &Offsets<'_>) -> Option<usize> {
    attrs
        .first()
        .map(|attr| at.byte(attr.pound_token.span.start()))
}

/// An attribute as written, a doc comment included.
fn attr_text<'t>(attr: &Attribute, text: &'t str, at: &Offsets<'_>) -> &'t str {
    let start = at.byte(attr.pound_token.span.start());
    let end = at
        .byte(attr.bracket_token.span.close().end())
        .max(at.byte(attr.pound_token.span.end()));
    &text[start..end]
}

/// Byte offsets of a text's lines, to place spans, whose columns count
/// characters.
struct Offsets<'t> {
    text: &'t str,
    lines: Vec<usize>,
}

impl<'t> Offsets<'t> {
    fn new(text: &'t str) -> Self {
        let lines = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(at, _)| at + 1))
            .collect();
        Self { text, lines }
    }

    /// The byte offset of `place`, its line counted from 1.
    fn byte(&self, place: LineColumn) -> usize {
        let line = self
            .lines
            .get(place.line.saturating_sub(1))
            .copied()
            .unwrap_or(self.text.len());
        let rest = &self.text[line..];
        line + rest
            .char_indices()
            .nth(place.column)
            .map_or(rest.len(), |(at, _)| at)
    }
}

#[cfg(test)]
mod tests {
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
        let peeled = peel(SOURCE)?;
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
    fn a_new_forwarder_file_names_load_and_its_source_s_imports() {
        let text = header(&["use core::time::Duration;".to_owned()]);
        assert!(text.ends_with("use core::time::Duration;\nuse crate::Load;\nuse crate::Off;\n"));
        let text = header(&["use crate::{Host, Off};".to_owned()]);
        assert!(text.ends_with("use crate::{Host, Off};\nuse crate::Load;\n"));
    }

    #[test]
    fn other_impls_stay_and_a_load_block_keeps_its_consts_and_types() -> Result<(), String> {
        let source = "impl Other {\n    pub fn f(&self) {}\n}\n\nimpl Clone for Load<'_> {\n    fn clone(&self) -> Self {\n        todo!()\n    }\n}\n\nimpl Load<'_> {\n    /// A part.\n    pub const PART: u32 = 1;\n\n    fn inner(&self) {}\n}\n";
        let peeled = peel(source)?;
        assert!(peeled.forwarders.is_empty());
        assert!(
            peeled.base.starts_with(
                "impl Other {\n    pub fn f(&self) {}\n}\n\nimpl Clone for Load<'_> {"
            )
        );
        assert!(peeled.base.ends_with(
            "impl BaseLoad {\n    /// A part.\n    pub const PART: u32 = 1;\n\n    fn inner(&self) {}\n}\n"
        ));
        Ok(())
    }

    #[test]
    fn a_script_adapter_moves_public_and_keeps_its_visibility_on_load() -> Result<(), String> {
        let source = "#[cfg(feature = \"unstable\")]\n#[crate::script(texts)]\nimpl Load<'_> {\n    /// Texts for a script.\n    #[script(name = \"texts\")]\n    pub(crate) fn texts_script(&mut self) -> u32 {\n        1\n    }\n}\n";
        let peeled = peel(source)?;
        assert_eq!(
            peeled.base,
            "#[cfg(feature = \"unstable\")]\nimpl BaseLoad {\n    /// As `Load::texts_script`.\n    #[doc(hidden)]\n    pub fn texts_script(&mut self) -> u32 {\n        1\n    }\n}\n"
        );
        assert_eq!(
            peeled.forwarders,
            [
                "#[enshwick_macros::forward(base)]\n#[crate::script(texts)]\nimpl Load<'_> {\n    /// Texts for a script.\n    #[script(name = \"texts\")]\n    pub(crate) fn texts_script(&mut self) -> u32;\n}"
            ]
        );
        Ok(())
    }
}
