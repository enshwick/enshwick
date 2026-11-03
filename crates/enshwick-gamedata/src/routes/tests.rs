use iced_x86::{Decoder, DecoderOptions, Instruction};

use super::shape::{self, Step};
use super::{Routed, Routes};
use crate::reflect::{Image, Layout, Reflection};
use crate::{Side, Table, embedded, exact};

#[test]
fn the_embedded_routes_read() {
    let routes = Routes::embedded().unwrap();
    assert!(routes.ids().count() > 0);
}

#[test]
fn a_route_without_exactly_one_way_or_with_a_wrong_take_is_refused() {
    for bad in [
        r#"id = "a""#,
        r#"id = "a"
           called_by = ["s"]
           loads = ["t"]"#,
        r#"id = "a"
           called_by = []"#,
        r#"id = "a"
           system = "s""#,
        r#"id = "a"
           loads = ["t"]
           take = "end""#,
        r#"id = "a"
           system = "s"
           shape = ["call ?"]
           take = "value""#,
        r#"id = "a"
           system = "s"
           shape = ["lea rdx,[rbp-*]"]"#,
        r#"id = "a"
           system = "s"
           shape = ["frobnicate rax"]"#,
        r#"id = "a"
           system = "s"
           shape = ["mov ecx,[rip]"]"#,
    ] {
        assert!(
            Routes::parse(&format!("[[route]]\n{bad}")).is_err(),
            "{bad}"
        );
    }
    let twice = "[[route]]\nid = \"a\"\nloads = [\"t\"]\n[[route]]\nid = \"a\"\nloads = [\"u\"]";
    assert!(Routes::parse(twice).is_err());
}

#[test]
fn a_route_is_limited_to_the_sides_it_names() {
    let routes = Routes::parse(
        "[[route]]\nid = \"a\"\nloads = [\"t\"]\nsides = [\"client\"]\n[[route]]\nid = \"b\"\nloads = [\"u\"]",
    )
    .unwrap();
    assert!(routes.routes("a", Side::Client));
    assert!(!routes.routes("a", Side::Server));
    assert!(routes.routes("b", Side::Server) && routes.routes("b", Side::Client));
}

/// `mov r8d, 0x1b8; lea rdx, [rbp-0x60]; mov rcx, r13; call +0; mov ecx,
/// [rip+0x10]; call -0x0a`, at 0x1000: the calls go to 0x1012 and 0x1013.
const CODE: [u8; 29] = [
    0x41, 0xb8, 0xb8, 0x01, 0x00, 0x00, 0x48, 0x8d, 0x55, 0xa0, 0x49, 0x8b, 0xcd, 0xe8, 0x00, 0x00,
    0x00, 0x00, 0x8b, 0x0d, 0x10, 0x00, 0x00, 0x00, 0xe8, 0xf6, 0xff, 0xff, 0xff,
];

fn decoded() -> Vec<Instruction> {
    let mut decoder = Decoder::with_ip(64, &CODE, 0x1000, DecoderOptions::NONE);
    let mut out = Vec::new();
    while decoder.can_decode() {
        out.push(decoder.decode());
    }
    out
}

fn steps(text: &[&str]) -> Vec<Step> {
    text.iter().map(|s| Step::parse(s).unwrap()).collect()
}

#[test]
fn a_shape_matches_what_the_instructions_do_and_takes_its_hole() {
    let none = |_: &str| None;
    let found = shape::find(
        &steps(&["lea rdx,[rbp-*]", "mov rcx,r13", "call ?"]),
        &decoded(),
        &none,
    )
    .unwrap();
    assert_eq!(
        (found.start, found.end, found.taken),
        (0x1006, 0x1012, Some(0x60))
    );
    let rip = shape::find(&steps(&["mov ecx,[rip+*]"]), &decoded(), &none).unwrap();
    assert_eq!(rip.taken, Some(0x1018 + 0x10));
    assert!(shape::find(&steps(&["lea rdx,[rbp+?]"]), &decoded(), &none).is_none());
    assert!(shape::find(&steps(&["mov rcx,r12"]), &decoded(), &none).is_none());
}

#[test]
fn a_shape_names_another_entry_by_its_address() {
    let at = |want: u64| move |id: &str| (id == "f.callee").then_some(want);
    let shape = steps(&["call {f.callee}"]);
    let found = shape::find(&shape, &decoded(), &at(0x1012)).unwrap();
    assert_eq!((found.start, found.end), (0x100d, 0x1012));
    let second = shape::find(&shape, &decoded(), &at(0x1013)).unwrap();
    assert_eq!(second.start, 0x1018);
    assert!(shape::find(&shape, &decoded(), &at(0x2000)).is_none());
}

/// On the executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name (each
/// skipped without it), every route of their side finds, with no table
/// behind it, the value their own table pins.
#[test]
#[cfg_attr(miri, ignore = "reading a game executable takes hours under Miri")]
fn every_route_finds_what_the_own_table_pins() {
    let tables: Vec<Table> = embedded()
        .iter()
        .map(|(_, text)| Table::parse(text).unwrap())
        .collect();
    let routes = Routes::embedded().unwrap();
    let empty = Table::parse(
        "name = \"empty\"\nside = \"server\"\nguid = \"00000000000000000000000000000000\"\nage = 0",
    )
    .unwrap()
    .resolve();
    for (var, side) in [
        ("ENSH_SERVER_EXE", Side::Server),
        ("ENSH_CLIENT_EXE", Side::Client),
    ] {
        let Ok(path) = std::env::var(var) else {
            continue;
        };
        let bytes = std::fs::read(path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        let own = exact(&tables, side, image.fingerprint().unwrap())
            .unwrap()
            .resolve();
        let wrong: Vec<String> = routes
            .find(&image, &reflection, side, &empty)
            .into_iter()
            .filter_map(|(id, routed)| {
                let pinned = match routed {
                    Some(Routed::Constant(_)) => own.constant(id).map(Routed::Constant),
                    _ => own.symbol(id).map(Routed::Symbol),
                };
                (routed.is_none() || routed != pinned)
                    .then(|| format!("{id}: route {routed:x?}, table {pinned:x?}"))
            })
            .collect();
        assert!(
            wrong.is_empty(),
            "{var} ({}):\n{}",
            own.name(),
            wrong.join("\n")
        );
    }
}
