//! The Luau definitions of the API, `enshwick.d.luau`, rendered from the
//! script descriptions the `enshwick` crate keeps beside its Rust items, for
//! luau-lsp and other Luau tools.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write as _;

use enshwick::script::{self, Choice, Class, Declared, Record, Signature, Ty, Union};

/// The file name editors are pointed at.
pub const DEFINITIONS_FILE: &str = "enshwick.d.luau";

/// The file name of the typed API's types, which editors load beside
/// [`DEFINITIONS_FILE`].
pub const TYPED_DEFINITIONS_FILE: &str = "enshwick.typed.d.luau";

/// The Rust path every generated type of the typed API starts with.
const TYPED: &str = "enshwick::typed::";

/// The words Luau keeps for itself, which a table type names only in
/// brackets.
const LUAU_KEYWORDS: [&str; 22] = [
    "and", "break", "continue", "do", "else", "elseif", "end", "false", "for", "function", "if",
    "in", "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// A field's key in a table type: its name, or `["name"]` for a keyword or
/// a name that is no identifier, such as a game field named `function`.
fn field_key(name: &str) -> Cow<'_, str> {
    let ident = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if ident && !LUAU_KEYWORDS.contains(&name) {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(format!("[{name:?}]"))
    }
}

/// The Luau name of the type named `name`: a generated type of the typed
/// API by its path there, `typed_ecs_Health` for `ecs::Health`, since every
/// Luau type shares one namespace; any other by its own name.
fn luau_name(name: &str) -> Cow<'_, str> {
    match name.strip_prefix(TYPED) {
        Some(path) => {
            let path = path.strip_prefix("root::").unwrap_or(path);
            Cow::Owned(format!("typed_{}", path.replace("::", "_")))
        }
        None => Cow::Borrowed(name),
    }
}

/// A named type the definitions declare.
enum Named {
    Record(&'static Record),
    Choice(&'static Choice),
    Object(&'static Class),
    Union(&'static Union),
}

impl Named {
    /// The type's declaration, with its doc.
    fn write(&self, out: &mut String) {
        match self {
            Self::Record(record) => {
                comment(record.doc(), "", out);
                let _written = writeln!(out, "export type {} = {{", luau_name(record.name()));
                for field in record.fields() {
                    comment(field.doc(), "\t", out);
                    let ty = render(field.ty());
                    let ty = if field.has_default() && !ty.ends_with('?') {
                        format!("{ty}?")
                    } else {
                        ty
                    };
                    let _written = writeln!(out, "\t{}: {ty},", field_key(field.name()));
                }
                out.push_str("}\n");
            }
            Self::Object(class) => {
                comment(class.doc(), "", out);
                let _written = writeln!(out, "export type {} = {{", class.name());
                for method in class.methods() {
                    comment(method.doc(), "\t", out);
                    let params = function(method.signature());
                    let _written = writeln!(
                        out,
                        "\t{}: (self: {}{}{},",
                        method.name(),
                        class.name(),
                        if params.starts_with("()") { "" } else { ", " },
                        &params[1..]
                    );
                }
                out.push_str("}\n");
            }
            Self::Union(union) => {
                comment(union.doc(), "", out);
                let variants: Vec<String> = union
                    .variants()
                    .iter()
                    .map(|v| match v.ty() {
                        Some(ty) => format!("{{ {}: {} }}", v.name(), render(ty)),
                        None => format!("{:?}", v.name()),
                    })
                    .collect();
                let _written = writeln!(
                    out,
                    "export type {} = {}",
                    luau_name(union.name()),
                    variants.join(" | ")
                );
            }
            Self::Choice(choice) => {
                comment(choice.doc(), "", out);
                let variants: Vec<String> = choice.names().map(|n| format!("{n:?}")).collect();
                let _written = writeln!(
                    out,
                    "export type {} = {}",
                    choice.name(),
                    variants.join(" | ")
                );
            }
        }
    }
}

/// A function of a namespace as the definitions declare it: callable any
/// time, or only while the mod starts.
struct Entry {
    name: &'static str,
    doc: &'static str,
    signature: &'static Signature,
    starting: bool,
}

impl Entry {
    /// The entry's line in a table type at `indent`, with its doc.
    fn write(&self, indent: &str, out: &mut String) {
        comment(self.doc, indent, out);
        if self.starting {
            let _written = writeln!(
                out,
                "{indent}--- Only while the mod starts, at the top level of main.luau."
            );
        }
        let _written = writeln!(out, "{indent}{}: {},", self.name, function(self.signature));
    }
}

/// Every namespace by name, with all its parts' functions, in the order
/// [`script::NAMESPACES`] first names each; the empty name is `enshwick`'s
/// own functions.
fn namespaces() -> Vec<(&'static str, &'static str, Vec<Entry>)> {
    let mut merged: Vec<(&'static str, &'static str, Vec<Entry>)> = Vec::new();
    for namespace in script::NAMESPACES {
        let known = merged
            .iter()
            .position(|(name, ..)| *name == namespace.name());
        let at = known.unwrap_or_else(|| {
            merged.push((namespace.name(), "", Vec::new()));
            merged.len() - 1
        });
        let (_, doc, entries) = &mut merged[at];
        if doc.is_empty() {
            *doc = namespace.doc();
        }
        entries.extend(namespace.functions().iter().map(|f: &Declared| Entry {
            name: f.name(),
            doc: f.doc(),
            signature: f.signature(),
            starting: false,
        }));
        entries.extend(namespace.init_functions().map(|f| Entry {
            name: f.name(),
            doc: f.doc(),
            signature: f.signature(),
            starting: true,
        }));
        // The getter a script calls at start before the namespace's functions.
        entries.extend(namespace.request_function().map(|f| Entry {
            name: f.name(),
            doc: f.doc(),
            signature: f.signature(),
            starting: true,
        }));
    }
    merged
}

/// Collects every named type inside `ty`.
fn collect(ty: &'static Ty, named: &mut BTreeMap<&'static str, Named>) {
    match *ty {
        Ty::Optional(inner) | Ty::List(inner) => collect(inner, named),
        Ty::Record(record) => {
            if named.insert(record.name(), Named::Record(record)).is_none() {
                for field in record.fields() {
                    collect(field.ty(), named);
                }
            }
        }
        Ty::Choice(choice) => {
            named.insert(choice.name(), Named::Choice(choice));
        }
        Ty::Function(signature) => collect_signature(signature, named),
        Ty::Union(union) if named.insert(union.name(), Named::Union(union)).is_none() => {
            for ty in union.variants().iter().filter_map(script::Variant::ty) {
                collect(ty, named);
            }
        }
        Ty::Object(class) if named.insert(class.name(), Named::Object(class)).is_none() => {
            for method in class.methods() {
                collect_signature(method.signature(), named);
            }
        }
        _ => {}
    }
}

/// Collects every named type of `signature`'s parameters and answer.
fn collect_signature(signature: &'static Signature, named: &mut BTreeMap<&'static str, Named>) {
    for param in signature.params() {
        collect(param.ty(), named);
    }
    collect(signature.ret(), named);
}

/// `ty` as a Luau type.
fn render(ty: &Ty) -> String {
    match *ty {
        Ty::Bool => "boolean".to_owned(),
        Ty::Number => "number".to_owned(),
        Ty::String => "string".to_owned(),
        Ty::Vector => "vector".to_owned(),
        Ty::Optional(inner) => match inner {
            Ty::Function(_) => format!("({})?", render(inner)),
            _ => format!("{}?", render(inner)),
        },
        Ty::List(inner) => format!("{{{}}}", render(inner)),
        Ty::Record(record) => luau_name(record.name()).into_owned(),
        Ty::Choice(choice) => choice.name().to_owned(),
        Ty::Function(signature) => function(signature),
        Ty::Data => "any".to_owned(),
        Ty::Bytes => "buffer".to_owned(),
        Ty::Object(class) => class.name().to_owned(),
        Ty::Union(union) => luau_name(union.name()).into_owned(),
        _ => "nil".to_owned(),
    }
}

/// What a function returns, `()` for nothing.
fn returns(ty: &Ty) -> String {
    match ty {
        Ty::Nil => "()".to_owned(),
        other => render(other),
    }
}

/// `signature` as a Luau function type.
fn function(signature: &Signature) -> String {
    let params: Vec<String> = signature
        .params()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let name = if p.name().is_empty() {
                format!("arg{}", i + 1)
            } else {
                p.name().to_owned()
            };
            format!("{name}: {}", render(p.ty()))
        })
        .collect();
    format!("({}) -> {}", params.join(", "), returns(signature.ret()))
}

/// `doc` as Luau doc comment lines at `indent`, with rustdoc links reduced
/// to their text.
fn comment(doc: &str, indent: &str, out: &mut String) {
    // Luau reaches `Clock::now` as `Clock.now`.
    let text = plain(doc).replace("::", ".");
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let _written = writeln!(out, "{indent}--- {line}");
    }
}

/// `doc` without rustdoc link targets, `[`X`](path)` and `[`X`]` become
/// `` `X` ``, and with Luau's words for Rust's values: `` `None` `` is nil,
/// `` `Some(x)` `` and `` `Ok(x)` `` are `` `x` ``.
fn plain(doc: &str) -> String {
    let mut out = String::with_capacity(doc.len());
    let mut rest = doc;
    while let Some(start) = rest.find('[') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        let Some(end) = after.find(']') else {
            out.push_str(&rest[start..]);
            return luau_words(&out);
        };
        out.push_str(&after[..end]);
        rest = &after[end + 1..];
        if let Some(target) = rest.strip_prefix('(')
            && let Some(close) = target.find(')')
        {
            rest = &target[close + 1..];
        }
    }
    out.push_str(rest);
    luau_words(&out)
}

/// `text` with `` `None` `` as nil and `` `Some(x)` `` and `` `Ok(x)` ``
/// as `` `x` ``.
fn luau_words(text: &str) -> String {
    let mut out = text.replace("`None`", "nil");
    for wrapper in ["`Some(", "`Ok("] {
        while let Some(start) = out.find(wrapper) {
            let inner = start + wrapper.len();
            let Some(close) = out[inner..].find(")`") else {
                break;
            };
            out.replace_range(
                start..inner + close + 2,
                &format!("`{}`", &out[inner..inner + close]),
            );
        }
    }
    out
}

/// Every named type the definitions declare, by name.
fn named() -> BTreeMap<&'static str, Named> {
    let mut named = BTreeMap::new();
    for (_, _, entries) in namespaces() {
        for entry in entries {
            collect_signature(entry.signature, &mut named);
        }
    }
    for event in script::events() {
        collect(event.data(), &mut named);
    }
    for event in script::game_events() {
        collect(event.data(), &mut named);
    }
    named
}

/// The names of the types [`definitions`] exports, which are global in
/// luau-lsp, so other definitions loaded beside them keep clear of them.
///
/// # Examples
///
/// ```
/// assert!(cargo_enshwick::luau::type_names().any(|name| name == "Player"));
/// ```
pub fn type_names() -> impl Iterator<Item = &'static str> {
    named().into_keys()
}

/// The types of the values `enshwick.typed` hands a script, a file of their
/// own beside [`definitions`].
///
/// A type for each generated struct and enum, which a script names where it
/// helps: `local value: typed_ecs_Health = health:read(entity)`.
///
/// # Examples
///
/// ```
/// let text = cargo_enshwick::luau::typed_definitions();
/// assert!(text.contains("export type typed_ecs_Health = {"));
/// ```
#[must_use]
pub fn typed_definitions() -> String {
    let mut named = BTreeMap::new();
    for ty in enshwick::typed::SCRIPT_TYPES {
        collect(ty, &mut named);
    }
    let mut out = String::from(
        "--!strict\n-- The types of the values of `enshwick.typed`, the game's components, events and\n-- resources, generated by `cargo enshwick` from the API's own descriptions; do not edit.\n",
    );
    for ty in named.values() {
        out.push('\n');
        ty.write(&mut out);
    }
    out
}

/// The whole definitions file.
///
/// # Examples
///
/// ```
/// let text = cargo_enshwick::luau::definitions();
/// assert!(text.contains("declare enshwick: {"));
/// assert!(text.contains("players: () -> {Player},"));
/// ```
#[must_use]
pub fn definitions() -> String {
    let named = named();
    let mut out = String::from(
        "--!strict\n-- The Enshwick API for Luau mods, generated by `cargo enshwick` from the\n-- API's own descriptions; do not edit.\n",
    );
    for ty in named.values() {
        out.push('\n');
        ty.write(&mut out);
    }
    out.push_str("\ndeclare enshwick: {\n");
    out.push_str("\t--- Calls `handler` on every loader or typed game event named `event`, with its data; subscribe at the top level of main.luau. A game value the API does not name arrives as `Other(<number>)`, a name of ours.\n\ton: ");
    let handler = |data: &Ty| match data {
        Ty::Nil => "() -> ()".to_owned(),
        data => format!("(data: {}) -> ()", render(data)),
    };
    let overloads: Vec<String> = script::events()
        .map(|event| (event.name(), handler(event.data())))
        .chain(script::game_events().map(|event| (event.name(), handler(event.data()))))
        .map(|(name, handler)| format!("((event: {name:?}, handler: {handler}) -> ())"))
        .collect();
    out.push_str(&overloads.join("\n\t\t& "));
    out.push_str(",\n");
    out.push_str("\t--- Calls `handler` on every game event of the reflected type `type_name`, with the fields `fields` names, each read as its kind: u8, i8, u16, i16, u32, i32, u64, i64, f32, f64, bool or entity; 64-bit values as strings. Subscribe at the top level of main.luau.\n\ton_game_type: (type_name: string, fields: { [string]: string }, handler: (event: { [string]: any }) -> ()) -> (),\n");
    out.push_str("\t--- Every game event type of the running build, with its size in bytes, read from the game's reflection.\n\tgame_event_types: () -> { { name: string, size: number } },\n");
    out.push_str("\t--- Links `handler` into another mod's extension point, as `cargo enshwick deps <alias> --luau` describes it: it gets the input and the answer so far, and returns a new answer, nothing to keep it, or `enshwick.refuse`. `order` places the link among the point's others: `after` a mod sees its answer, `before` a mod leaves it the last word, and among the rest the lower `priority` runs first. Subscribe at the top level of main.luau.\n\textend: (point: { name: string }, handler: (input: any, answer: any) -> any, order: { before: { string }?, after: { string }?, priority: number? }?) -> (),\n");
    out.push_str(
        "\t--- What a point's handler returns to refuse, on a refusable point.\n\trefuse: {},\n",
    );
    out.push_str("\t--- Calls `handler` with the data of another mod's event, as `cargo enshwick deps <alias> --luau` describes it or a description of the script's own (fields with `name` and `read`, laid out as Rust lays them out). Subscribe at the top level of main.luau.\n\ton_mod_event: (event: { name: string }, handler: (data: any) -> ()) -> (),\n");
    out.push_str("\t--- The function that runs another mod's call, as `cargo enshwick deps <alias> --luau` describes it: it takes the input and returns the answer, or nil and why not (`unmatched` also before the start is over, `off` or `failed`). Ask at the top level of main.luau. Unstable: may change in any release.\n\tcaller: (call: { name: string }) -> (input: any) -> (any, string?),\n");
    out.push_str("\t--- Defines a mod event of this mod for others to hear, at the top level of main.luau, by its local name (`greeted` for `<mod id>:greeted`); `send` hands every subscriber the data.\n\tdefine_event: (event: { name: string }) -> { send: (data: any) -> () },\n");
    out.push_str("\t--- The extension points this mod owns, which `pack.toml` declares under `[[point]]` for other mods to extend.\n\tpoints: {\n\t\t--- Defines the point `pack.toml` declares by the local name `name` (`price` for `<mod id>:price`), at the top level of main.luau; other mods' links run once every mod has started. `decide` runs them over this mod's own answer and returns the answer with `\"kept\"` or `\"changed\"`, or nil with `\"refused\"`; `is_extended` says whether a link runs.\n\t\tdefine_point: (name: string) -> {\n\t\t\tdecide: (self: any, input: { [string]: any }, answer: any) -> (any, \"kept\" | \"changed\" | \"refused\"),\n\t\t\tis_extended: (self: any) -> boolean,\n\t\t},\n\t},\n");
    out.push_str("\t--- Serves a call of this mod for other mods, at the top level of main.luau, by its local name (`price-of` for `<mod id>:price-of`), as a Rust mod serves a `#[call]` type: the input's fields and the answer read numbers, booleans, entities or GUIDs, laid out as Rust lays them out, so a Rust caller of the same type matches. `handler` gets the input and returns the answer; nil fails the call.\n\tserve_call: (call: { name: string, major: number?, fields: { { name: string, read: string } }, output: string }, handler: (input: { [string]: any }) -> any) -> (),\n");
    out.push_str("\t--- Calls `handler` with the mod's settings now and as each world loads and stops, as a Rust mod's `on_settings` does.\n\ton_settings: (handler: (settings: { [string]: any }) -> ()) -> (),\n");
    out.push_str("\t--- Calls `handler` as `on_settings` does, and again on every change in game, as a Rust mod's `on_settings_change` does.\n\ton_settings_change: (handler: (settings: { [string]: any }) -> ()) -> (),\n");
    out.push_str("\t--- The mods `pack.toml` names in `requires` and `after`, by alias: each mod's id, and whether it runs now.\n\tdeps: { [string]: { id: string, running: () -> boolean } },\n");
    out.push_str("\t--- The mod's settings in force, as its `pack.toml` declares them under `[[setting]]`; read them again in a `SettingsChanged` handler.\n\tsettings: () -> { [string]: any },\n");
    for (name, doc, entries) in namespaces() {
        if name.is_empty() {
            for entry in &entries {
                entry.write("\t", &mut out);
            }
            continue;
        }
        comment(doc, "\t", &mut out);
        let _written = writeln!(out, "\t{name}: {{");
        for entry in &entries {
            entry.write("\t\t", &mut out);
        }
        out.push_str("\t},\n");
    }
    out.push_str("}\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_lose_their_targets() {
        assert_eq!(
            plain("see [`World::notify`] and [`Load::on`](crate::Load::on)."),
            "see `World::notify` and `Load::on`."
        );
        assert_eq!(plain("a [b"), "a [b");
    }

    #[test]
    fn no_two_types_share_a_name() {
        let doc = |named: &Named| match named {
            Named::Record(r) => r.doc(),
            Named::Choice(c) => c.doc(),
            Named::Union(u) => u.doc(),
            Named::Object(o) => o.doc(),
        };
        let mut first: BTreeMap<&str, &str> = BTreeMap::new();
        let mut clashes = Vec::new();
        let signatures = namespaces()
            .into_iter()
            .flat_map(|(_, _, entries)| entries.into_iter().map(|e| e.signature));
        for signature in signatures {
            let mut one = BTreeMap::new();
            collect_signature(signature, &mut one);
            for (name, named) in &one {
                let known = *first.entry(name).or_insert_with(|| doc(named));
                if known != doc(named) {
                    clashes.push(*name);
                }
            }
        }
        clashes.sort_unstable();
        clashes.dedup();
        assert!(clashes.is_empty(), "types of one name: {clashes:?}");
    }

    #[test]
    fn rust_values_read_as_luau_s() {
        assert_eq!(
            plain("`None` off the thread, `Some(true)` or `Ok(false)` else."),
            "nil off the thread, `true` or `false` else."
        );
        assert_eq!(plain("`Some(x` stays"), "`Some(x` stays");
    }

    #[test]
    fn the_definitions_speak_no_rust() {
        let rust = [
            "::", "`None`", "`Some", "`Ok", "`Err", "Option<", "Result<", "Vec<",
        ];
        for line in definitions()
            .lines()
            .filter(|l| l.trim_start().starts_with("---"))
        {
            assert!(!rust.iter().any(|word| line.contains(word)), "{line}");
        }
    }

    #[test]
    fn the_definitions_declare_every_namespace_function_and_event() {
        let text = definitions();
        for namespace in script::NAMESPACES {
            // The namespace without a name is the `enshwick` table itself.
            let indent = if namespace.name().is_empty() {
                "\t"
            } else {
                assert!(
                    text.contains(&format!("\t{}: {{", namespace.name())),
                    "{}",
                    namespace.name()
                );
                "\t\t"
            };
            let names = namespace
                .functions()
                .iter()
                .map(Declared::name)
                .chain(namespace.init_functions().map(script::InitFunction::name))
                .chain(namespace.request_function().map(|f| f.name()))
                .collect::<Vec<_>>();
            for name in names {
                assert!(text.contains(&format!("{indent}{name}: (")), "{name}");
            }
        }
        for event in script::events() {
            assert!(
                text.contains(&format!("event: {:?}", event.name())),
                "{}",
                event.name()
            );
        }
    }

    #[test]
    fn every_covered_type_has_a_luau_type_of_its_own_name() {
        let text = typed_definitions();
        let mut names = std::collections::BTreeSet::new();
        for line in text.lines() {
            if let Some(name) = line
                .strip_prefix("export type ")
                .and_then(|rest| rest.split(' ').next())
            {
                assert!(
                    name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'),
                    "{name}"
                );
                assert!(names.insert(name.to_owned()), "{name} declared twice");
            }
        }
        let script = enshwick::typed::SCRIPT_TYPES.iter();
        for (row, ty) in enshwick::typed::NAMED.iter().zip(script) {
            let Ty::Record(record) = ty else {
                panic!("{}: no record", row.id);
            };
            let name = luau_name(record.name());
            assert!(names.contains(name.as_ref()), "{} has no Luau type", row.id);
        }
        assert!(
            enshwick::typed::NAMED.len() > 900,
            "{} covered types",
            enshwick::typed::NAMED.len()
        );
        let main: Vec<&str> = type_names().collect();
        assert!(
            names.iter().all(|name| !main.contains(&name.as_str())),
            "a typed type takes a name of the API's"
        );
    }

    #[test]
    fn a_generated_type_is_named_by_its_path_in_the_typed_api() {
        assert_eq!(
            luau_name("enshwick::typed::ecs::Health"),
            "typed_ecs_Health"
        );
        assert_eq!(luau_name("enshwick::typed::root::Guid"), "typed_Guid");
        assert_eq!(luau_name("Player"), "Player");
    }

    #[test]
    fn a_field_named_after_a_luau_keyword_is_written_in_brackets() {
        assert_eq!(field_key("health"), "health");
        assert_eq!(field_key("function"), "[\"function\"]");
        assert_eq!(field_key("2d"), "[\"2d\"]");
        assert!(
            !typed_definitions().contains("\tfunction:"),
            "a keyword field reads as a field"
        );
    }

    #[test]
    fn the_typed_definitions_speak_no_rust() {
        let rust = ["::", "`None`", "`Some", "Option<", "Vec<"];
        for line in typed_definitions()
            .lines()
            .filter(|l| l.trim_start().starts_with("---"))
        {
            assert!(!rust.iter().any(|word| line.contains(word)), "{line}");
        }
    }

    #[test]
    fn the_definitions_match_their_snapshot() {
        insta::assert_snapshot!(definitions());
    }

    #[test]
    fn every_example_pack_carries_the_current_definitions() {
        let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples")
            .canonicalize()
            .unwrap();
        let fresh = definitions();
        let mut checked = 0;
        for pack in std::fs::read_dir(&examples).unwrap() {
            let copy = pack.unwrap().path().join("types").join(DEFINITIONS_FILE);
            if let Ok(text) = std::fs::read_to_string(&copy) {
                assert!(
                    text == fresh,
                    "{0} is stale: write it with `cargo enshwick luau --out {0}`",
                    copy.display()
                );
                checked += 1;
            }
        }
        assert_ne!(checked, 0, "no example pack carries {DEFINITIONS_FILE}");
    }
}
