//! The ranges and alignments that bound a route's constant, the data an
//! address it takes must lie in, and the type a field route's last step
//! holds.

use super::super::{is_name_hash, proposed_range};
use super::*;

/// Rows over [`fake`] that bound what [`FAKE_ROUTES`] takes: the same
/// displacement inside and outside a range, a shifted 8 that is not a
/// multiple of 16, and an address taken in `.pdata`.
const BOUNDED_ROUTES: &str = r#"
[[route]]
id = "t.disp_in"
in = "t.leaf"
shape = ["mov eax,[rcx+*]", "shl eax,?"]
take = "value"
plus = 1
range = [0x70, 0x80]

[[route]]
id = "t.disp_out"
in = "t.leaf"
shape = ["mov eax,[rcx+*]", "shl eax,?"]
take = "value"
plus = 1
range = [0x70, 0x78]

[[route]]
id = "t.shift_odd"
in = "t.leaf"
shape = ["mov eax,[rcx+0x78]", "shl eax,*"]
take = "value"
shift = true
align = 16

[[route]]
id = "t.unwind"
in = "sys_g"
shape = ["lea rcx,[rip+*]"]
take = "address"
"#;

#[test]
fn a_constant_outside_its_range_or_an_address_outside_the_data_is_no_entry() {
    let mut fake = fake();
    let mut unwind = vec![0x48, 0x8d, 0x0d];
    unwind.extend(rel(0x1087, PDATA + 0x800));
    unwind.push(0xc3);
    fake.function(0x1080, &unwind, Some(6));
    let (name, record) = (RDATA + 0x160, RDATA + 0x2c0);
    fake.put(name, b"sys_g\0");
    for (k, word) in [BASE + name, 5, BASE + 0x1080].into_iter().enumerate() {
        fake.put(record + k as u64 * 8, &word.to_le_bytes());
    }
    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let routes = Routes::parse(&format!("{FAKE_ROUTES}{BOUNDED_ROUTES}")).unwrap();
    let found = routes.find(&image, &reflection, Side::Server, &empty());
    let of = |id: &str| found.iter().find(|(have, _)| *have == id).map(|&(_, f)| f);
    assert_eq!(of("t.disp_in"), Some(Some(Routed::Constant(0x79))));
    assert_eq!(of("t.disp_out"), Some(Some(Routed::Outside(0x79))));
    assert_eq!(of("t.shift_odd"), Some(Some(Routed::Outside(8))));
    assert_eq!(of("t.unwind"), Some(None), "an address in .pdata");
    assert_eq!(
        of("t.string"),
        Some(Some(Routed::Reached(0x2180))),
        "one in .rdata"
    );
}

/// A constant from code or reflection that nothing bounds counts as soon
/// as a shape matches a changed instruction once, so each states the
/// values it may take, and each field route the type its last field holds.
#[test]
fn every_constant_route_states_its_range() {
    let routes = Routes::embedded().unwrap();
    let unbounded: Vec<&str> = routes
        .routes
        .iter()
        .filter(|r| r.is_entry() && r.finds_constant() && r.range.is_none())
        .map(|r| r.id.as_str())
        .collect();
    assert!(unbounded.is_empty(), "no `range`: {unbounded:?}");
    let untyped: Vec<&str> = routes
        .routes
        .iter()
        .filter(|r| matches!(r.how, How::Field(_)) && r.holds.is_none())
        .map(|r| r.id.as_str())
        .collect();
    assert!(
        untyped.is_empty(),
        "a field route without `type`: {untyped:?}"
    );
}

#[test]
fn a_proposed_range_keeps_a_name_hash_exact_and_leaves_room_for_an_offset() {
    assert_eq!(proposed_range(0x40, 0x60), (0x20, 0xc0));
    assert_eq!(proposed_range(0, 3), (0, 16), "at least 16");
    assert_eq!(
        proposed_range(0x724b_f568, 0x724b_f568),
        (0x724b_f568, 0x724b_f568)
    );
    assert_eq!(
        proposed_range(1 << 60, 1 << 62),
        (0, u64::MAX),
        "a key that changes"
    );
    assert_eq!(
        proposed_range(0x7b_c028, 0xeb_c028),
        (0x3d_e014, 0x1d7_8050)
    );
    assert_eq!(
        proposed_range(0x46_3e98, 0x46_3e98),
        (0x23_1f4c, 0x8c_7d30),
        "an offset into a big object that never moved"
    );
}

/// A struct offset fixed to one value turns its feature off as soon as a
/// build moves the struct, so only a name hash keeps an exact range.
#[test]
fn only_a_name_hash_has_an_exact_range() {
    let routes = Routes::embedded().unwrap();
    let exact: Vec<String> = routes
        .routes
        .iter()
        .filter_map(|r| {
            r.range
                .filter(|&(lo, hi)| lo == hi && !is_name_hash(lo))
                .map(|(lo, _)| format!("{}: {lo:#x}", r.id))
        })
        .collect();
    assert!(
        exact.is_empty(),
        "exact ranges that are no name hash: {exact:?}"
    );
}

#[test]
fn a_range_bounds_only_a_constant_and_reads_least_first() {
    let row = |extra: &str| {
        Routes::parse(&format!(
            "[[route]]\nid = \"t.x\"\nsystem = \"sys\"\nshape = [\"mov eax,[rcx+*]\", \"ret\"]\n{extra}"
        ))
    };
    assert!(row("take = \"value\"\nrange = [1, 2]\nalign = 1").is_ok());
    assert!(
        row("take = \"value\"\nrange = [2, 1]").is_err(),
        "greatest first"
    );
    assert!(row("take = \"value\"\nalign = 0").is_err());
    assert!(
        row("take = \"value\"\nunbounded = true").is_ok(),
        "a 64-bit key, past what a TOML integer holds"
    );
    assert!(
        row("take = \"value\"\nunbounded = true\nrange = [1, 2]").is_err(),
        "both"
    );
    assert!(
        row("take = \"address\"\nrange = [1, 2]").is_err(),
        "a symbol"
    );
    let reflected = |extra: &str| {
        Routes::parse(&format!(
            "[[route]]\nid = \"t.f\"\nfield = [\"keen::A.b\"]\n{extra}"
        ))
    };
    assert!(reflected("range = [0, 0x100]\ntype = \"keen::uint32\"").is_ok());
    assert!(
        row("take = \"value\"\ntype = \"keen::uint32\"").is_err(),
        "a type for a field route only"
    );
}
