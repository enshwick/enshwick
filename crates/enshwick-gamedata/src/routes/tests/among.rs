//! Routes that list several functions in `callers` or `callees`.

use super::{RDATA, empty, fake, rel};
use crate::Side;
use crate::reflect::{Image, Layout, Reflection};
use crate::routes::{How, Kin, Routed, Routes};

/// A route that lists several functions passes over one nothing finds, so
/// each it lists must be a route of the table, else a misspelt name leaves
/// the entry resting on the others alone.
#[test]
fn every_function_a_route_lists_is_a_route() {
    let routes = Routes::embedded().unwrap();
    let ids: Vec<&str> = routes.routes.iter().map(|r| r.id.as_str()).collect();
    for route in &routes.routes {
        if let How::Among {
            kin: Kin::Callers(names) | Kin::Callees(names),
            ..
        } = &route.how
            && names.len() > 1
        {
            for name in names {
                assert!(ids.contains(&name.as_str()), "{}: {name}", route.id);
            }
        }
    }
}

/// A `callees` route over several functions gives what each found one
/// gives, all agreeing: `~fn:a` and `sys_b` each call only the leaf; `c`
/// calls `a` and `b`, neither of which reads like the leaf, and is passed
/// over, as is a name nothing finds; `~fn:x`, which calls only the run
/// `mov eax,5; ret`, finds another `ret` than `~fn:a` does, so the two give
/// nothing, unless the route's check refuses the one `~fn:a` finds. `c`'s
/// callees hold `ret` twice, so a route listing `c` never guesses.
#[test]
fn a_route_over_several_functions_gives_what_every_found_one_agrees_on() {
    let mut fake = fake();
    let world = RDATA + 0x190;
    fake.put(world, b"world\0");
    let mut x = vec![0x48, 0x8d, 0x0d];
    x.extend(rel(0x1087, world));
    x.push(0xe8);
    x.extend(rel(0x108c, 0x1062));
    x.push(0xc3);
    fake.function(0x1080, &x, Some(6));
    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let row = |id: &str, callers: &str, shape: &str| {
        format!("[[route]]\nid = \"{id}\"\ncallees = {callers}\nshape = [\"{shape}\"]\n")
    };
    let leaf = |id: &str, callers: &str| row(id, callers, "mov eax,[rcx+?]");
    let text = [
        leaf("t.agreed", r#"["~fn:a", "sys_b"]"#),
        leaf("t.one_absent", r#"["t.absent", "~fn:a", "sys_b"]"#),
        leaf("t.passed_over", r#"["t.c", "sys_b"]"#),
        leaf("t.none_found", r#"["t.absent", "t.nowhere"]"#),
        row("t.apart", r#"["~fn:a", "~fn:x"]"#, "ret"),
        row("t.alone", r#"["~fn:x"]"#, "ret"),
        row("t.refused", r#"["~fn:a", "~fn:x"]"#, "ret") + "check = [\"mov eax,0x5\", \"ret\"]\n",
        row("t.blocked", r#"["t.c", "~fn:x"]"#, "ret"),
        "[[route]]\nid = \"~fn:a\"\nloads = [\"hello\"]\n".to_owned(),
        "[[route]]\nid = \"~fn:x\"\nloads = [\"world\"]\n".to_owned(),
        "[[route]]\nid = \"sys_b\"\nsystem = \"sys_b\"\n".to_owned(),
        "[[route]]\nid = \"t.c\"\ncalls = [\"~fn:a\", \"sys_b\"]\n".to_owned(),
    ]
    .concat();
    let routes = Routes::parse(&text).unwrap();
    let found: Vec<(&str, Option<Routed>)> =
        routes.find(&image, &reflection, Side::Server, &empty());
    assert_eq!(
        found,
        [
            ("t.agreed", Some(Routed::Reached(0x1040))),
            ("t.alone", Some(Routed::Reached(0x1062))),
            ("t.apart", None),
            ("t.blocked", None),
            ("t.c", Some(Routed::Reached(0x1020))),
            ("t.none_found", None),
            ("t.one_absent", Some(Routed::Reached(0x1040))),
            ("t.passed_over", Some(Routed::Reached(0x1040))),
            ("t.refused", Some(Routed::Symbol(0x1062))),
        ]
    );
    for bad in [row("t.empty", "[]", "ret"), row("t.number", "[1]", "ret")] {
        assert!(Routes::parse(&bad).is_err(), "{bad}");
    }
}
