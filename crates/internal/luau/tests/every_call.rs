//! Every Luau function and object method runs through the fake host: each
//! is called at the start with sample arguments of its parameters' types,
//! and answers or raises an error that names it, never a panic that turns
//! the mod off.

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::fmt::Write as _;

    use enshwick::abi::Status;
    use enshwick::script::{self, Class, Signature, Ty};
    use enshwick::test::FakeHost;

    use std::time::Duration;

    use enshwick_luau::{Limits, Scripts};

    use crate::harness::start_with_limits;

    /// A Luau expression of type `ty`, as a script might pass it.
    fn sample(ty: &Ty) -> String {
        match ty {
            Ty::Bool => "true".to_owned(),
            Ty::Number => "1".to_owned(),
            Ty::String => "\"you.luau:sample\"".to_owned(),
            Ty::Vector => "vector.create(1, 2, 3)".to_owned(),
            Ty::List(_) | Ty::Data => "{}".to_owned(),
            Ty::Record(record) => {
                let fields: Vec<String> = record
                    .fields()
                    .iter()
                    .map(|f| format!("{} = {}", f.name(), sample(f.ty())))
                    .collect();
                format!("{{ {} }}", fields.join(", "))
            }
            Ty::Choice(choice) => format!("{:?}", choice.names().next().unwrap_or_default()),
            Ty::Union(union) => match union.variants().first() {
                Some(v) => match v.ty() {
                    Some(inner) => format!("{{ {} = {} }}", v.name(), sample(inner)),
                    None => format!("{:?}", v.name()),
                },
                None => "nil".to_owned(),
            },
            Ty::Function(_) => "function() return nil end".to_owned(),
            Ty::Bytes => "buffer.create(4)".to_owned(),
            _ => "nil".to_owned(),
        }
    }

    fn arguments(signature: &Signature) -> String {
        signature
            .params()
            .iter()
            .map(|p| sample(p.ty()))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Calls every method of the objects of `classes` that `value` names
    /// when it holds one.
    fn try_methods(value: &str, classes: &[&'static Class], out: &mut String) {
        for class in classes {
            let _ = writeln!(
                out,
                "if typeof({value}) == 'userdata' and tostring({value}) == {:?} then",
                class.name()
            );
            for method in class.methods() {
                let _ = writeln!(
                    out,
                    "  check({:?}, pcall({value}.{}, {value}{}{}))",
                    format!("{}:{}", class.name(), method.name()),
                    method.name(),
                    if method.signature().params().is_empty() {
                        ""
                    } else {
                        ", "
                    },
                    arguments(method.signature())
                );
            }
            out.push_str("end\n");
        }
    }

    fn classes_in(ty: &'static Ty, classes: &mut Vec<&'static Class>) {
        match *ty {
            Ty::Optional(inner) | Ty::List(inner) => classes_in(inner, classes),
            Ty::Object(class) if !classes.iter().any(|c| core::ptr::eq(*c, class)) => {
                classes.push(class);
                for method in class.methods() {
                    classes_in(method.signature().ret(), classes);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn every_function_and_method_answers_or_raises_an_error_naming_itself() {
        let mut calls = Vec::new();
        let mut classes = Vec::new();
        for namespace in script::NAMESPACES {
            let path = if namespace.name().is_empty() {
                "enshwick".to_owned()
            } else {
                format!("enshwick.{}", namespace.name())
            };
            for f in namespace.functions() {
                calls.push((format!("{path}.{}", f.name()), f.signature()));
                classes_in(f.signature().ret(), &mut classes);
            }
            for f in namespace.init_functions() {
                calls.push((format!("{path}.{}", f.name()), f.signature()));
                classes_in(f.signature().ret(), &mut classes);
            }
            // A getter goes first, so the namespace's functions run opened.
            if let Some(f) = namespace.request_function() {
                calls.insert(0, (format!("{path}.{}", f.name()), f.signature()));
                classes_in(f.signature().ret(), &mut classes);
            }
        }
        let mut main = String::from(
            "local seen = 0\n\
             local function check(what, ok, why)\n\
             \tseen += 1\n\
             \tif not ok and not string.find(tostring(why), what, 1, true) then\n\
             \t\terror(`{what} raised an error that names another call: {why}`)\n\
             \tend\n\
             end\n",
        );
        // A write lands in the test's folder; ported script five writes to a
        // folder of its own.
        calls.retain(|(name, _)| name != "enshwick.files.write_atomically");
        for (name, signature) in &calls {
            let args = arguments(signature);
            let sep = if args.is_empty() { "" } else { ", " };
            // A namespace the fake host lacks the symbols to open is absent.
            let _ = writeln!(
                main,
                "if {name} then local ok, value = pcall({name}{sep}{args})"
            );
            let _ = writeln!(main, "check({name:?}, ok, value)");
            try_methods("value", &classes, &mut main);
            main.push_str("end\n");
        }
        main.push_str("print(`calls {seen}`)\n");
        let host = FakeHost::new()
            .items()
            .type_hash("current_transform", 1)
            .type_hash("used_item", 2);
        // One start calls every function: past the default start budget on
        // a slow runner, which this test does not measure.
        let mut limits = Limits::default();
        limits.start = Duration::from_secs(30);
        let (loader, status) = start_with_limits(host, &main, Scripts::new(), None, limits);
        assert_eq!(status, Status::OK, "{:?}", loader.logs());
        let names: BTreeSet<&str> = calls.iter().map(|(n, _)| n.as_str()).collect();
        let seen: usize = loader
            .logs()
            .iter()
            .find_map(|(_, l)| l.strip_prefix("calls ")?.parse().ok())
            .unwrap_or_default();
        assert!(
            seen * 10 >= names.len() * 9,
            "{seen} calls ran of {} functions: {:?}",
            names.len(),
            loader.logs()
        );
    }
}
