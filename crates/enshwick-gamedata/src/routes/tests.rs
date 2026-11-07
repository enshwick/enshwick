mod builds;

use iced_x86::{Decoder, DecoderOptions, Instruction};

use super::shape::{self, Step};
use super::{How, Routed, Routes};
use crate::reflect::{Image, Layout, Reflection};
use crate::{Side, Table, kept};

#[test]
fn the_embedded_routes_read() {
    let routes = Routes::embedded().unwrap();
    assert!(routes.ids().count() > 0);
}

/// Every type a `field` or `size` route names is in the typed catalogue of
/// each table build of its side that has a snapshot, and each field holds
/// the type of the step after it.
#[test]
#[cfg_attr(miri, ignore = "reads the typed snapshots from disk")]
fn every_reflected_name_a_route_takes_is_in_the_typed_catalogue() {
    let routes = Routes::embedded().unwrap();
    let mut checked = 0;
    for (_, text) in kept().unwrap() {
        let table = Table::parse(&text).unwrap();
        let Some(file) = crate::typed::snapshot_path(&table.file_name()) else {
            continue;
        };
        let Ok(snapshot) = std::fs::read_to_string(&file) else {
            continue;
        };
        let catalogue = crate::typed::Catalogue::from_snapshot(&snapshot).unwrap();
        for route in routes
            .routes
            .iter()
            .filter(|r| r.sides.contains(&table.side()))
        {
            let ty = |name: &str| {
                catalogue
                    .types
                    .get(name)
                    .unwrap_or_else(|| panic!("{}: {name} is not in {file:?}", route.id))
            };
            match &route.how {
                How::Field(steps) => {
                    for (at, (owner, field)) in steps.iter().enumerate() {
                        let held = ty(owner)
                            .fields
                            .iter()
                            .find(|f| f.name == *field)
                            .unwrap_or_else(|| panic!("{}: {owner}.{field} in {file:?}", route.id));
                        if let Some((next, _)) = steps.get(at + 1) {
                            assert_eq!(held.ty.as_ref(), Some(next), "{}", route.id);
                        }
                    }
                }
                How::Size(name) => {
                    ty(name);
                }
                _ => continue,
            }
            checked += 1;
        }
    }
    assert!(
        checked > 0 || crate::data_dir().is_none(),
        "no reflected route met a snapshot"
    );
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
           calls = []"#,
        r#"id = "a"
           system = "s"
           in = "t"
           shape = ["call ?"]"#,
        r#"id = "a"
           in = "t""#,
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
        r#"id = "a"
           in = "s"
           shape = ["call ?"]
           take = "fixed""#,
        r#"id = "a"
           in = "s"
           shape = ["call *"]
           take = "address"
           value = 1"#,
        r#"id = "a"
           in = "s"
           shape = ["call *"]
           take = "address"
           plus = 1"#,
        r#"id = "a"
           in = "s"
           shape = ["mov eax,[rcx+r10*3+?]"]"#,
        r#"id = "a"
           in = "s"
           shape = ["mov eax,[rcx+0x0]"]"#,
        r#"id = "a"
           called_by = ["s"]
           shape = ["ret"]"#,
        r#"id = "a"
           calls = ["s"]
           shape = ["ret"]"#,
        r#"id = "a"
           loads = ["s"]
           shape = ["ret"]"#,
        r#"id = "a"
           size = "keen::A"
           shape = ["ret"]"#,
        r#"id = "a"
           callers = "s""#,
        r#"id = "a"
           system = "s"
           shape = []"#,
    ] {
        assert!(
            Routes::parse(&format!("[[route]]\n{bad}")).is_err(),
            "{bad}"
        );
    }
    let twice = "[[route]]\nid = \"a\"\nloads = [\"t\"]\n[[route]]\nid = \"a\"\nloads = [\"u\"]";
    assert!(Routes::parse(twice).is_err());
    let system = Routes::parse("[[route]]\nid = \"a\"\nsystem = \"s\"").unwrap();
    assert_eq!(system.routes[0].how, How::System("s".to_owned()));
}

#[test]
fn an_id_has_one_route_per_side() {
    let routes = Routes::parse(
        "[[route]]\nid = \"a.b\"\nloads = [\"t\"]\nsides = [\"client\"]\n[[route]]\nid = \"a.b\"\nloads = [\"u\"]\nsides = [\"server\"]",
    )
    .unwrap();
    assert!(routes.routes("a.b", Side::Client) && routes.routes("a.b", Side::Server));
    assert_eq!(routes.ids().collect::<Vec<_>>(), ["a.b"]);
    let overlap = "[[route]]\nid = \"a.b\"\nloads = [\"t\"]\n[[route]]\nid = \"a.b\"\nloads = [\"u\"]\nsides = [\"server\"]";
    assert!(Routes::parse(overlap).is_err());
}

#[test]
fn a_route_is_limited_to_the_sides_it_names() {
    let routes = Routes::parse(
        "[[route]]\nid = \"a.x\"\nloads = [\"t\"]\nsides = [\"client\"]\n[[route]]\nid = \"b.x\"\nloads = [\"u\"]",
    )
    .unwrap();
    assert!(routes.routes("a.x", Side::Client));
    assert!(!routes.routes("a.x", Side::Server));
    assert!(routes.routes("b.x", Side::Server) && routes.routes("b.x", Side::Client));
}

#[test]
fn waypoints_are_no_entries() {
    let routes = Routes::parse(
        "[[route]]\nid = \"~fn:a.x\"\nloads = [\"t\"]\n[[route]]\nid = \"some_system\"\nsystem = \"some_system\"\n[[route]]\nid = \"a.x\"\nin = \"~fn:a.x\"\nshape = [\"ret\"]",
    )
    .unwrap();
    assert_eq!(routes.ids().collect::<Vec<_>>(), ["a.x"]);
    assert!(!routes.routes("~fn:a.x", Side::Server));
    assert!(!routes.routes("some_system", Side::Server));
}

/// `mov r8d, 0x1b8; lea rdx, [rbp-0x60]; mov rcx, r13; call +0; mov ecx,
/// [rip+0x10]; call -0x0a; mov eax, [rcx+r10*4+0x10]`, at 0x1000: the calls
/// go to 0x1012 and 0x1013.
const CODE: [u8; 34] = [
    0x41, 0xb8, 0xb8, 0x01, 0x00, 0x00, 0x48, 0x8d, 0x55, 0xa0, 0x49, 0x8b, 0xcd, 0xe8, 0x00, 0x00,
    0x00, 0x00, 0x8b, 0x0d, 0x10, 0x00, 0x00, 0x00, 0xe8, 0xf6, 0xff, 0xff, 0xff, 0x42, 0x8b, 0x44,
    0x91, 0x10,
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
    let names = shape::ShapeNames::new();
    text.iter()
        .map(|s| Step::parse(s, &names).unwrap())
        .collect()
}

fn first(text: &[&str], entry: &impl Fn(&str) -> Option<u64>) -> Option<shape::Match> {
    let steps = steps(text);
    let code = decoded();
    shape::matches(&steps, &code, entry).next()
}

#[test]
fn a_shape_matches_what_the_instructions_do_and_takes_its_hole() {
    let none = |_: &str| None;
    let found = first(&["lea rdx,[rbp-*]", "mov rcx,r13", "call ?"], &none).unwrap();
    assert_eq!(
        (found.start, found.end, found.taken),
        (0x1006, 0x1012, Some(0x60))
    );
    let rip = first(&["mov ecx,[rip+*]"], &none).unwrap();
    assert_eq!(rip.taken, Some(0x1018 + 0x10));
    assert!(first(&["lea rdx,[rbp+?]"], &none).is_none());
    assert!(first(&["mov rcx,r12"], &none).is_none());
}

#[test]
fn a_shape_matches_an_index_register_an_exact_displacement_and_an_exact_immediate() {
    let none = |_: &str| None;
    let indexed = first(&["mov eax,[rcx+r10*4+*]"], &none).unwrap();
    assert_eq!((indexed.start, indexed.taken), (0x101d, Some(0x10)));
    assert!(first(&["mov eax,[rcx+r10*4+0x10]"], &none).is_some());
    assert!(first(&["mov eax,[rcx+r10*4+0x18]"], &none).is_none());
    assert!(first(&["mov eax,[rcx+r10*8+?]"], &none).is_none());
    assert!(first(&["mov eax,[rcx+?]"], &none).is_none());
    assert!(first(&["lea rdx,[rbp-0x60]"], &none).is_some());
    assert!(first(&["lea rdx,[rbp+0x60]"], &none).is_none());
    assert!(first(&["mov r8d,0x1b8"], &none).is_some());
    assert!(first(&["mov r8d,0x1b9"], &none).is_none());
}

#[test]
fn a_shape_names_another_entry_by_its_address() {
    let at = |want: u64| move |id: &str| (id == "f.callee").then_some(want);
    let found = first(&["call {f.callee}"], &at(0x1012)).unwrap();
    assert_eq!((found.start, found.end), (0x100d, 0x1012));
    let second = first(&["call {f.callee}"], &at(0x1013)).unwrap();
    assert_eq!(second.start, 0x1018);
    assert!(first(&["call {f.callee}"], &at(0x2000)).is_none());
}

/// A PE32+ file with `.text`, `.rdata` and `.pdata`, each at the same
/// offset in the file as its RVA.
struct Fake {
    bytes: Vec<u8>,
}

const BASE: u64 = 0x1_4000_0000;
const TEXT: u64 = 0x1000;
const RDATA: u64 = 0x2000;
const PDATA: u64 = 0x3000;
const SIZE: u64 = 0x1000;

impl Fake {
    fn new() -> Self {
        let mut bytes = vec![0u8; usize::try_from(PDATA + SIZE).unwrap()];
        bytes[usize::try_from(TEXT).unwrap()..usize::try_from(TEXT + SIZE).unwrap()].fill(0xcc);
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes[0x86..0x88].copy_from_slice(&3u16.to_le_bytes());
        bytes[0x94..0x96].copy_from_slice(&0xf0u16.to_le_bytes());
        let optional = 0x98;
        bytes[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        bytes[optional + 24..optional + 32].copy_from_slice(&BASE.to_le_bytes());
        let headers = optional + 0xf0;
        for (i, (name, rva)) in [
            (&b".text\0\0\0"[..], TEXT),
            (&b".rdata\0\0"[..], RDATA),
            (&b".pdata\0\0"[..], PDATA),
        ]
        .into_iter()
        .enumerate()
        {
            let at = headers + i * 40;
            bytes[at..at + 8].copy_from_slice(name);
            for (field, value) in [(8, SIZE), (12, rva), (16, SIZE), (20, rva)] {
                let value = u32::try_from(value).unwrap();
                bytes[at + field..at + field + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
        Self { bytes }
    }

    fn put(&mut self, rva: u64, data: &[u8]) {
        let at = usize::try_from(rva).unwrap();
        self.bytes[at..at + data.len()].copy_from_slice(data);
    }

    /// Writes `code` at `rva`, with a `.pdata` row when `pdata` is set.
    fn function(&mut self, rva: u64, code: &[u8], row: Option<u64>) {
        self.put(rva, code);
        if let Some(row) = row {
            let end = rva + code.len() as u64;
            // The unwind info points at zeros: no chained parent.
            for (at, word) in [rva, end, RDATA + 0x800].into_iter().enumerate() {
                let word = u32::try_from(word).unwrap();
                self.put(PDATA + row * 12 + at as u64 * 4, &word.to_le_bytes());
            }
        }
    }
}

/// `rel32` from the instruction ending at `next` to `target`.
fn rel(next: u64, target: u64) -> [u8; 4] {
    i32::try_from(target.cast_signed() - next.cast_signed())
        .unwrap()
        .to_le_bytes()
}

/// Function `a` loads "hello" and calls the leaf; `b`, the code of system
/// record `sys_b`, calls the leaf and holds `mov eax, 7` twice; `c` calls
/// `a` and `b`; the leaf at 0x1040 has no `.pdata` row. `d` (`sys_d`) loads
/// an address inside `a`; `e` (`sys_e`) jumps to the unpadded run after it,
/// `f` (`sys_f`) has one after it it does not jump to; `sys_c` has two
/// records with different entries.
fn fake() -> Fake {
    let mut fake = Fake::new();
    let hello = RDATA + 0x180;
    fake.put(hello, b"hello\0");
    let mut loads = vec![0x48, 0x8d, 0x0d];
    loads.extend(rel(0x1007, hello));
    loads.push(0xe8);
    loads.extend(rel(0x100c, 0x1040));
    loads.push(0xc3);
    fake.function(0x1000, &loads, Some(0));
    let mut twice = vec![0xe8];
    twice.extend(rel(0x1015, 0x1040));
    twice.extend([0xb8, 7, 0, 0, 0, 0xb8, 7, 0, 0, 0, 0xc3]);
    fake.function(0x1010, &twice, Some(1));
    let mut calls = vec![0xe8];
    calls.extend(rel(0x1025, 0x1000));
    calls.push(0xe8);
    calls.extend(rel(0x102a, 0x1010));
    calls.push(0xc3);
    fake.function(0x1020, &calls, Some(2));
    fake.function(0x1040, &[0x8b, 0x41, 0x78, 0xc1, 0xe0, 0x03, 0xc3], None);
    let mut inside = vec![0x48, 0x8d, 0x05];
    inside.extend(rel(0x1057, 0x1005));
    inside.push(0xc3);
    fake.function(0x1050, &inside, Some(3));
    fake.function(0x1060, &[0xeb, 0x00], Some(4));
    fake.function(0x1062, &[0xb8, 5, 0, 0, 0, 0xc3], None);
    fake.function(0x1070, &[0x90, 0x90], Some(5));
    fake.function(0x1072, &[0xb8, 6, 0, 0, 0, 0xc3], None);
    let records: [(&[u8], u64); 6] = [
        (b"sys_b", 0x1010),
        (b"sys_d", 0x1050),
        (b"sys_e", 0x1060),
        (b"sys_f", 0x1070),
        (b"sys_c", 0x1000),
        (b"sys_c", 0x1020),
    ];
    for (i, (name, entry)) in records.into_iter().enumerate() {
        let i = i as u64;
        let at = RDATA + 0x100 + i * 0x10;
        fake.put(at, name);
        fake.put(at + name.len() as u64, &[0]);
        let record = RDATA + 0x200 + i * 0x20;
        for (k, word) in [BASE + at, name.len() as u64, BASE + entry]
            .into_iter()
            .enumerate()
        {
            fake.put(record + k as u64 * 8, &word.to_le_bytes());
        }
    }
    fake
}

/// Routes of every kind over [`fake`], each row before the ones it names.
const FAKE_ROUTES: &str = r#"
[[route]]
id = "t.spelled"
in = "t.leaf"
shape = ["mov eax,[rcx+?]", "shl eax,0x3"]
take = "spelled"
value = 3

[[route]]
id = "t.field"
field = ["keen::Missing.value"]

[[route]]
id = "t.called"
callers = "t.leaf"
shape = ["mov eax,0x7", "mov eax,0x7"]

[[route]]
id = "t.called_twice"
callers = "t.leaf"
shape = ["ret"]

[[route]]
id = "t.disp"
in = "t.leaf"
shape = ["mov eax,[rcx+*]", "shl eax,?"]
take = "value"
plus = 1

[[route]]
id = "t.shift"
in = "t.leaf"
shape = ["mov eax,[rcx+0x78]", "shl eax,*"]
take = "value"
shift = true

[[route]]
id = "t.leaf"
called_by = ["~fn:a", "sys_b"]

[[route]]
id = "t.checked"
called_by = ["~fn:a", "sys_b"]
check = ["mov eax,[rcx+0x78]", "shl eax,0x3", "ret"]

[[route]]
id = "t.unchecked"
called_by = ["~fn:a", "sys_b"]
check = ["mov eax,[rcx+0x70]"]

[[route]]
id = "t.caller"
calls = ["~fn:a", "sys_b"]

[[route]]
id = "t.twice"
in = "sys_b"
shape = ["mov eax,0x7"]

[[route]]
id = "t.once"
in = "sys_b"
shape = ["mov eax,0x7", "ret"]

[[route]]
id = "t.long"
in = "sys_b"
shape = ["call ?", "mov eax,0x7"]

[[route]]
id = "t.loop"
in = "t.loop"
shape = ["ret"]

[[route]]
id = "t.string"
in = "~fn:a"
shape = ["lea rcx,[rip+*]"]
take = "address"

[[route]]
id = "t.inside"
in = "sys_d"
shape = ["lea rax,[rip+*]"]
take = "address"

[[route]]
id = "t.ambiguous"
system = "sys_c"

[[route]]
id = "t.tail"
in = "sys_e"
shape = ["mov eax,0x5", "ret"]

[[route]]
id = "t.untail"
in = "sys_f"
shape = ["mov eax,0x6", "ret"]

[[route]]
id = "sys_b"
system = "sys_b"

[[route]]
id = "~fn:a"
loads = ["hello"]
"#;

const EMPTY: &str =
    "name = \"empty\"\nside = \"server\"\nguid = \"00000000000000000000000000000000\"\nage = 0";

fn empty() -> crate::Resolved {
    Table::parse(EMPTY).unwrap().resolve()
}

#[test]
fn every_kind_of_route_finds_its_entry_in_any_row_order() {
    let fake = fake();
    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let routes = Routes::parse(FAKE_ROUTES).unwrap();
    let found: Vec<(&str, Option<Routed>)> =
        routes.find(&image, &reflection, Side::Server, &empty());
    assert_eq!(
        found,
        [
            ("t.ambiguous", None),
            ("t.called", Some(Routed::Reached(0x1010))),
            ("t.called_twice", None),
            ("t.caller", Some(Routed::Reached(0x1020))),
            ("t.checked", Some(Routed::Symbol(0x1040))),
            ("t.disp", Some(Routed::Constant(0x79))),
            ("t.field", None),
            ("t.inside", None),
            ("t.leaf", Some(Routed::Reached(0x1040))),
            ("t.long", Some(Routed::Symbol(0x1010))),
            ("t.loop", None),
            ("t.once", Some(Routed::Reached(0x101a))),
            ("t.shift", Some(Routed::Constant(8))),
            ("t.spelled", Some(Routed::Constant(3))),
            ("t.string", Some(Routed::Reached(0x2180))),
            ("t.tail", Some(Routed::Reached(0x1062))),
            ("t.twice", None),
            ("t.unchecked", None),
            ("t.untail", None),
        ]
    );
    assert!(routes.checks("t.long", Side::Server));
    assert!(routes.checks("t.checked", Side::Server));
    assert!(!routes.checks("t.leaf", Side::Server));
}

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
/// values it may take.
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

/// In `sys_b`, `call; mov eax,7; mov eax,7; ret`: an ordinal takes the
/// `nth` of the identical moves, counted after its anchor when it has one,
/// and finds nothing past the last, where the anchor is not unique or where
/// the moves are not `of` many.
#[test]
fn an_ordinal_takes_the_nth_run_after_its_anchor() {
    let fake = fake();
    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let row = |id: &str, keys: &str| {
        format!("[[route]]\nid = \"{id}\"\nin = \"sys_b\"\nshape = [\"mov eax,0x7\"]\n{keys}\n")
    };
    let text = [
        row("t.second", "nth = 2\nof = 2"),
        row("t.miscounted", "nth = 1\nof = 3"),
        row("t.after", "after = [\"call ?\"]\nnth = 1\nof = 2"),
        row("t.past", "after = [\"call ?\"]\nnth = 3\nof = 3"),
        row("t.anchors", "after = [\"mov eax,0x7\"]\nnth = 1\nof = 1"),
        row("t.named", "after = [\"call {t.leaf}\"]\nnth = 2\nof = 2"),
        "[[route]]\nid = \"t.unique\"\nin = \"sys_b\"\nshape = [\"call {t.leaf}\", \"mov eax,0x7\"]\n".to_owned(),
        "[[route]]\nid = \"~fn:a\"\nloads = [\"hello\"]\n".to_owned(),
        "[[route]]\nid = \"t.leaf\"\ncalled_by = [\"~fn:a\", \"sys_b\"]\n".to_owned(),
    ]
    .concat();
    let routes = Routes::parse(&text).unwrap();
    let found: Vec<(&str, Option<Routed>)> =
        routes.find(&image, &reflection, Side::Server, &empty());
    assert_eq!(
        found,
        [
            ("t.after", Some(Routed::Reached(0x1015))),
            ("t.anchors", None),
            ("t.leaf", Some(Routed::Reached(0x1040))),
            ("t.miscounted", None),
            ("t.named", Some(Routed::Reached(0x101a))),
            ("t.past", None),
            ("t.second", Some(Routed::Reached(0x101a))),
            ("t.unique", Some(Routed::Symbol(0x1010))),
        ]
    );
    for bad in [
        row("t.zero", "nth = 0\nof = 2"),
        row("t.uncounted", "nth = 1"),
        row("t.counted", "of = 2"),
        row("t.beyond", "nth = 3\nof = 2"),
        row("t.alone", "after = [\"call ?\"]"),
        row("t.taking", "after = [\"call *\"]\nnth = 1\nof = 2"),
        "[[route]]\nid = \"t.callers\"\ncallers = \"t.leaf\"\nshape = [\"ret\"]\nnth = 1\nof = 1\n"
            .to_owned(),
    ] {
        assert!(Routes::parse(&bad).is_err(), "{bad}");
    }
}

#[test]
fn a_memory_operand_through_any_base_matches_every_register_but_rip() {
    let none = |_: &str| None;
    let any = first(&["lea rdx,[?-*]"], &none).unwrap();
    assert_eq!((any.start, any.taken), (0x1006, Some(0x60)));
    assert!(first(&["mov eax,[?+r10*4+0x10]"], &none).is_some());
    assert!(first(&["mov ecx,[?+?]"], &none).is_none());
}

/// A slot route takes the function a vtable slot points at, only where the
/// pointer starts a function and the route's check holds; a slot row names
/// its vtable and slot and nothing else.
#[test]
fn a_slot_route_takes_the_function_a_vtable_slot_points_at() {
    let mut fake = fake();
    let vtable = RDATA + 0x300;
    fake.put(vtable, &(BASE + 0x1010).to_le_bytes());
    fake.put(vtable + 8, &(BASE + 0x1005).to_le_bytes());
    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let known = Table::parse(&format!("{EMPTY}\n[symbols]\n\"t.vtable\" = {vtable:#x}\n"))
        .unwrap()
        .resolve();
    let routes = Routes::parse(
        r#"
        [[route]]
        id = "t.first"
        vtable = "t.vtable"
        slot = 0

        [[route]]
        id = "t.checked"
        vtable = "t.vtable"
        slot = 0
        check = ["call ?", "mov eax,0x7"]

        [[route]]
        id = "t.inside"
        vtable = "t.vtable"
        slot = 1

        [[route]]
        id = "t.wrong"
        vtable = "t.vtable"
        slot = 0
        check = ["ret"]
        "#,
    )
    .unwrap();
    assert_eq!(
        routes.find(&image, &reflection, Side::Server, &known),
        [
            ("t.checked", Some(Routed::Symbol(0x1010))),
            ("t.first", Some(Routed::Reached(0x1010))),
            ("t.inside", None),
            ("t.wrong", None),
        ]
    );
    assert!(routes.checks("t.checked", Side::Server));
    for row in [
        "vtable = \"t.vtable\"",
        "slot = 1",
        "vtable = \"t.vtable\"\nslot = 1\nshape = [\"ret\"]",
        "vtable = \"t.vtable\"\nslot = 1\nloads = [\"x\"]",
    ] {
        let error = Routes::parse(&format!("[[route]]\nid = \"t.x\"\n{row}\n")).unwrap_err();
        assert!(error.to_string().contains("slot"), "{row}: {error}");
    }
}

#[test]
fn nothing_builds_on_a_route_the_table_contradicts() {
    let fake = fake();
    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let routes = Routes::parse(FAKE_ROUTES).unwrap();
    let known = Table::parse(&format!("{EMPTY}\n[symbols]\n\"t.leaf\" = 0x1000"))
        .unwrap()
        .resolve();
    let found = routes.find(&image, &reflection, Side::Server, &known);
    let of = |id: &str| found.iter().find(|(f, _)| *f == id).unwrap().1;
    assert_eq!(of("t.leaf"), Some(Routed::Reached(0x1040)));
    assert_eq!(of("t.disp"), None);
    assert_eq!(of("t.shift"), None);
}

#[test]
fn a_loose_constant_a_fixed_value_or_a_taking_check_is_refused() {
    let row = |keys: &str| format!("[[route]]\nid = \"a.b\"\nin = \"s\"\n{keys}");
    for bad in [
        "shape = [\"mov eax,[rcx+*]\"]\ntake = \"value\"",
        "shape = [\"mov eax,[rip+*]\"]\ntake = \"cell\"",
        "shape = [\"mov eax,?\"]\ntake = \"fixed\"\nvalue = 4",
        "shape = [\"call *\"]\ntake = \"address\"\ncheck = [\"mov eax,*\"]",
        "shape = [\"mov eax,[rcx+?]\", \"ret\"]\ntake = \"value\"\ncheck = [\"ret\"]",
    ] {
        assert!(Routes::parse(&row(bad)).is_err(), "{bad}");
    }
    for good in [
        "shape = [\"mov eax,[rcx+*]\", \"ret\"]\ntake = \"value\"",
        "shape = [\"cmp [rcx+0x10],*\"]\ntake = \"value\"",
        "shape = [\"lea rcx,{a.c}\", \"mov eax,[rcx+*]\"]\ntake = \"value\"",
    ] {
        assert!(Routes::parse(&row(good)).is_ok(), "{good}");
    }
}

#[test]
fn a_spelled_value_needs_a_step_that_spells_it_and_a_reflected_number_stands_alone() {
    let spelled = |keys: &str| format!("[[route]]\nid = \"a.b\"\nin = \"s\"\n{keys}");
    for bad in [
        "shape = [\"mov eax,[rcx+?]\", \"ret\"]\ntake = \"spelled\"\nvalue = 0",
        "shape = [\"mov eax,[rcx+0x10]\"]\ntake = \"spelled\"\nvalue = 0x10",
        "shape = [\"mov eax,[rcx+0x10]\"]\ntake = \"spelled\"\nvalue = 1",
        "shape = [\"mov eax,[rcx]\"]\ntake = \"spelled\"",
        "shape = [\"mov eax,[rcx+*]\"]\ntake = \"spelled\"\nvalue = 0",
        "shape = [\"mov eax,[rip+?]\"]\ntake = \"spelled\"\nvalue = 0",
        "shape = [\"mov eax,[rcx]\", \"ret\"]\ntake = \"spelled\"\nvalue = 0\ncheck = [\"ret\"]",
        "shape = [\"mov eax,[rcx]\"]\ntake = \"spelled\"\nvalue = 0",
        "shape = [\"nop [rax]\", \"cmp [rdx+?],r8d\"]\ntake = \"spelled\"\nvalue = 0",
        "shape = []\ntake = \"spelled\"\nvalue = 0",
    ] {
        assert!(Routes::parse(&spelled(bad)).is_err(), "{bad}");
    }
    for good in [
        "shape = [\"mov eax,[rcx]\", \"ret\"]\ntake = \"spelled\"\nvalue = 0",
        "shape = [\"lea eax,[rcx+rdx*4]\", \"ret\"]\ntake = \"spelled\"\nvalue = 4",
        "shape = [\"mov eax,0x18\"]\ntake = \"spelled\"\nvalue = 0x18",
    ] {
        assert!(Routes::parse(&spelled(good)).is_ok(), "{good}");
    }
    let reflected = |keys: &str| format!("[[route]]\nid = \"a.b\"\n{keys}");
    for bad in [
        "field = [\"keen::A\"]",
        "field = [\"keen::A.b\"]\ntake = \"value\"",
        "field = [\"keen::A.b\"]\nshape = [\"ret\"]",
        "size = \"keen::A\"\ncheck = [\"ret\"]",
        "size = \"keen::A\"\nfield = [\"keen::A.b\"]",
    ] {
        assert!(Routes::parse(&reflected(bad)).is_err(), "{bad}");
    }
    for good in [
        "field = [\"keen::A.b\", \"keen::B.c\"]",
        "size = \"keen::A\"",
    ] {
        assert!(Routes::parse(&reflected(good)).is_ok(), "{good}");
    }
}

/// The record sizes of the game's code, each held only in index arithmetic,
/// fold to their factor; what the fold cannot follow gives none.
#[test]
fn a_scale_folds_the_factors_of_one_index_and_refuses_what_it_cannot_follow() {
    let scale = |bytes: &[u8]| {
        let run: Vec<Instruction> = Decoder::with_ip(64, bytes, 0x1000, DecoderOptions::NONE)
            .into_iter()
            .collect();
        shape::scale(&run)
    };
    // lea r9,[rax+rax*4]; shl r9,5; cmp [r9+r8],edx
    assert_eq!(
        scale(&[
            0x4c, 0x8d, 0x0c, 0x80, 0x49, 0xc1, 0xe1, 0x05, 0x43, 0x39, 0x14, 0x01
        ]),
        Some(160)
    );
    // lea r10,[rax+rax*2]; cmp byte [rcx+r10*4+0x10],0
    assert_eq!(
        scale(&[0x4c, 0x8d, 0x14, 0x40, 0x42, 0x80, 0x7c, 0x91, 0x10, 0x00]),
        Some(12)
    );
    // lea edx,[rax+8]; mov rax,[rcx+0x10]; add rdx,rdx; mov eax,[rax+rdx*8]
    assert_eq!(
        scale(&[
            0x8d, 0x50, 0x08, 0x48, 0x8b, 0x41, 0x10, 0x48, 0x03, 0xd2, 0x8b, 0x04, 0xd0
        ]),
        Some(16)
    );
    // imul rax,rcx,0x48; mov eax,[rdx+rax]
    assert_eq!(
        scale(&[0x48, 0x6b, 0xc1, 0x48, 0x8b, 0x04, 0x02]),
        Some(0x48)
    );
    // lea rcx,[rcx+rcx*4]; shl rcx,4: the register the last one writes
    assert_eq!(
        scale(&[0x48, 0x8d, 0x0c, 0x89, 0x48, 0xc1, 0xe1, 0x04]),
        Some(80)
    );
    // mov rax,rcx; shl rax,4; sub rax,rcx; mov eax,[rdx+rax]
    assert_eq!(
        scale(&[
            0x48, 0x89, 0xc8, 0x48, 0xc1, 0xe0, 0x04, 0x48, 0x29, 0xc8, 0x8b, 0x04, 0x02
        ]),
        Some(15)
    );
    // cmp rcx,[rip+0x10]; lea rax,[rcx+rcx*2]; mov eax,[rdx+rax*4]: RIP is no index
    assert_eq!(
        scale(&[
            0x48, 0x3b, 0x0d, 0x10, 0, 0, 0, 0x48, 0x8d, 0x04, 0x49, 0x8b, 0x04, 0x82
        ]),
        Some(12)
    );
    // lea r9,[rax+rax*4]; xor ecx,ecx; mov eax,[rdx+r9]: a write of another register
    assert_eq!(
        scale(&[0x4c, 0x8d, 0x0c, 0x80, 0x31, 0xc9, 0x42, 0x8b, 0x04, 0x0a]),
        Some(5)
    );
    // lea rax,[rcx+rcx*2]; movss xmm0,[rdx+rax*4]: the last load needs no fold
    assert_eq!(
        scale(&[0x48, 0x8d, 0x04, 0x49, 0xf3, 0x0f, 0x10, 0x04, 0x82]),
        Some(12)
    );
    // lea rax,[rcx+rcx*4]; movzx eax,al: a byte truncates the multiple
    assert_eq!(scale(&[0x48, 0x8d, 0x04, 0x89, 0x0f, 0xb6, 0xc0]), None);
    // lea rcx,[rcx+rcx*4]; mov al,cl: a byte write keeps the rest of rax
    assert_eq!(scale(&[0x48, 0x8d, 0x0c, 0x89, 0x88, 0xc8]), None);
    // lea eax,[rcx+rcx*4]; shl eax,0x21: the processor shifts by 1, not 33
    assert_eq!(scale(&[0x8d, 0x04, 0x89, 0xc1, 0xe0, 0x21]), None);
    // lea rax,[rcx+rcx*2]; shl rax,cl: a shift by a register
    assert_eq!(scale(&[0x48, 0x8d, 0x04, 0x49, 0x48, 0xd3, 0xe0]), None);
    // lea rax,[rcx+rdx]: the first instruction reads two registers
    assert_eq!(scale(&[0x48, 0x8d, 0x04, 0x11]), None);
    // mov eax,[rcx]: a factor of 1 multiplies nothing
    assert_eq!(scale(&[0x8b, 0x01]), None);
    // lea r9,[rax+rax*4]; imul r9,rcx: a product of two registers
    assert_eq!(
        scale(&[0x4c, 0x8d, 0x0c, 0x80, 0x4c, 0x0f, 0xaf, 0xc9]),
        None
    );
    // lea r9,[rax+rax*4]; call; mov eax,[r9]: the call may change r9
    assert_eq!(
        scale(&[0x4c, 0x8d, 0x0c, 0x80, 0xe8, 0, 0, 0, 0, 0x41, 0x8b, 0x01]),
        None
    );
    let route = |keys: &str| Routes::parse(&format!("[[route]]\nid = \"a.b\"\nin = \"s\"\n{keys}"));
    assert!(route("shape = [\"lea r9,[rax+rax*4]\", \"shl r9,0x5\"]\ntake = \"scale\"").is_ok());
    for bad in [
        "shape = [\"lea r9,[rax+rax*4]\", \"shl r9,*\"]\ntake = \"scale\"",
        "shape = [\"lea r9,[rax+rax*4]\"]\ntake = \"scale\"",
        "shape = [\"lea r9,[rax+rax*4]\", \"shl r9,0x5\"]\ntake = \"scale\"\nplus = 1",
        "shape = [\"lea r9,[rax+rax*4]\", \"shl r9,0x5\"]\ntake = \"scale\"\nvalue = 0xa0",
    ] {
        assert!(route(bad).is_err(), "{bad}");
    }
}

/// An anchor row stands only for what the routes leave: an entry no route
/// of its side finds, a symbol whose route does not check its code, a
/// symbol a kept constant reads through, or the `.bytes` of a kept symbol.
#[test]
fn the_anchors_hold_only_what_no_route_replaces() {
    let routes = Routes::embedded().unwrap();
    for side in [Side::Server, Side::Client] {
        let anchors = crate::anchors(side).unwrap();
        let read_through: Vec<&str> = anchors
            .constants()
            .iter()
            .filter_map(crate::Constant::via)
            .collect();
        let kept: Vec<&str> = anchors.symbols().iter().map(crate::Symbol::id).collect();
        let stale: Vec<&str> = anchors
            .symbols()
            .iter()
            .map(crate::Symbol::id)
            .filter(|id| {
                routes.routes(id, side) && routes.checks(id, side) && !read_through.contains(id)
            })
            .chain(
                anchors
                    .constants()
                    .iter()
                    .map(crate::Constant::id)
                    .filter(|id| match id.strip_suffix(".bytes") {
                        Some(symbol) => !kept.contains(&symbol),
                        None => routes.routes(id, side),
                    }),
            )
            .collect();
        assert!(stale.is_empty(), "{side:?}: {stale:?}");
    }
}
