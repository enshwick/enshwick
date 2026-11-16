//! The script descriptions stay language-neutral: every namespace a runtime
//! opens reads as plain named types and functions, so a generator for
//! another language (a WIT world, C# bindings) reads the same descriptions
//! the Luau runtime does.

use std::collections::{BTreeMap, BTreeSet};

use crate::script::{NAMESPACES, Signature, Ty};

/// Which way a value crosses: in from the script, where a function is a
/// callback it passes, which every language can export; or out to it, where
/// a function would be a closure the host hands over, which only a script
/// holds.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Side {
    In,
    Out,
}

/// The walk over every description, with what it found wrong.
#[derive(Default)]
struct Walk {
    /// Each named type's name and its shape, its doc and its members' names,
    /// so two different types under one name show; one type's description
    /// may lie at several addresses, as a constant does.
    named: BTreeMap<&'static str, String>,
    /// The named types walked on each side.
    walked: BTreeSet<(&'static str, Side)>,
    faults: Vec<String>,
}

/// Whether `name` is an identifier every target language takes.
fn identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// A named type's shape: its doc and its members' names.
fn shape<'a>(doc: &str, members: impl Iterator<Item = &'a str>) -> String {
    members.fold(doc.to_owned(), |shape, member| shape + "\n" + member)
}

impl Walk {
    fn name(&mut self, what: &str, name: &str) {
        if !identifier(name) {
            self.faults
                .push(format!("{what} `{name}` is not an identifier"));
        }
    }

    /// Notes the named type `name` of `shape`, crossing on `side`; true
    /// when this side of it is new.
    fn named(&mut self, name: &'static str, shape: String, side: Side) -> bool {
        if !name.split("::").all(identifier) {
            self.faults
                .push(format!("type `{name}` is not a path of identifiers"));
        }
        if let Some(before) = self.named.insert(name, shape)
            && before != self.named[name]
        {
            self.faults
                .push(format!("two types share the name `{name}`"));
        }
        self.walked.insert((name, side))
    }

    /// Walks `signature`. A named function's or method's parameters come in
    /// from the script and need names; a callback's are what the host gives
    /// it, positional in every language, and its answer comes in.
    fn signature(&mut self, at: &str, signature: &'static Signature, callback: bool) {
        let (given, answered) = if callback {
            (Side::Out, Side::In)
        } else {
            (Side::In, Side::Out)
        };
        for param in signature.params() {
            if !callback {
                self.name(&format!("a parameter of {at}"), param.name());
            }
            self.ty(at, param.ty(), given);
        }
        self.ty(at, signature.ret(), answered);
    }

    fn ty(&mut self, at: &str, ty: &'static Ty, side: Side) {
        match *ty {
            Ty::Optional(inner) | Ty::List(inner) => self.ty(at, inner, side),
            Ty::Function(signature) => {
                if side == Side::Out {
                    self.faults
                        .push(format!("{at} hands the script a function"));
                }
                self.signature(at, signature, true);
            }
            Ty::Record(record) => {
                let fields = record
                    .fields()
                    .iter()
                    .map(enshwick_values::script::Field::name);
                if self.named(record.name(), shape(record.doc(), fields), side) {
                    for field in record.fields() {
                        self.name(&format!("a field of {}", record.name()), field.name());
                        self.ty(record.name(), field.ty(), side);
                    }
                }
            }
            Ty::Choice(choice) => {
                if self.named(choice.name(), shape(choice.doc(), choice.names()), side) {
                    for name in choice.names() {
                        self.name(&format!("a name of {}", choice.name()), name);
                    }
                }
            }
            Ty::Union(union) => {
                let variants = union
                    .variants()
                    .iter()
                    .map(enshwick_values::script::Variant::name);
                if self.named(union.name(), shape(union.doc(), variants), side) {
                    for variant in union.variants() {
                        self.name(&format!("a variant of {}", union.name()), variant.name());
                        if let Some(held) = variant.ty() {
                            self.ty(union.name(), held, side);
                        }
                    }
                }
            }
            Ty::Object(class) => {
                let methods = class.methods();
                let names = methods.iter().map(enshwick_values::script::Declared::name);
                // A method's signature is the same whichever way the handle
                // crossed: walk it on one side only.
                if self.named(class.name(), shape(class.doc(), names), Side::In) {
                    for method in &methods {
                        self.name(&format!("a method of {}", class.name()), method.name());
                        self.signature(class.name(), method.signature(), false);
                    }
                }
            }
            _ => {}
        }
    }
}

#[test]
fn every_script_description_reads_in_any_language() {
    let mut walk = Walk::default();
    for namespace in NAMESPACES {
        if !namespace.name().is_empty() {
            walk.name("a namespace", namespace.name());
        }
        let functions = namespace
            .functions()
            .into_iter()
            .chain(namespace.request_function())
            .map(|f| (f.name(), f.signature()))
            .chain(
                namespace
                    .init_functions()
                    .map(|f| (f.name(), f.signature())),
            );
        for (name, signature) in functions {
            let at = format!("{}.{name}", namespace.name());
            walk.name(&format!("function {at}"), name);
            walk.signature(&at, signature, false);
        }
    }
    assert!(walk.named.len() > 50, "the walk reached the named types");
    assert!(walk.faults.is_empty(), "{:#?}", walk.faults);
}

#[test]
fn a_function_handed_to_the_script_and_a_name_with_a_dot_are_faults() {
    static CALLBACK: Signature = Signature::new(&[], &Ty::Nil);
    static FUNCTION: Ty = Ty::Function(&CALLBACK);
    static LISTED: Ty = Ty::List(&FUNCTION);
    let mut walk = Walk::default();
    walk.ty("test", &LISTED, Side::Out);
    walk.name("a field", "you.shrine");
    assert_eq!(walk.faults.len(), 2, "{:#?}", walk.faults);
    walk.faults.clear();
    walk.ty("test", &LISTED, Side::In);
    assert!(
        walk.faults.is_empty(),
        "callbacks the script passes read anywhere"
    );
}
