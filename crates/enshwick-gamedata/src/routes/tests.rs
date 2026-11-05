use iced_x86::{Decoder, DecoderOptions, Instruction};

use super::shape::{self, Step};
use super::{How, Routed, Routes};
use crate::reflect::{Image, Layout, Reflection};
use crate::{Side, Table, embedded, exact};

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
    for (_, text) in embedded() {
        let table = Table::parse(text).unwrap();
        let file = std::path::Path::new(&table.file_name()).with_extension("jsonl");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("typed")
            .join(&file);
        let Ok(snapshot) = std::fs::read_to_string(path) else {
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
    assert!(checked > 0, "no reflected route met a snapshot");
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

/// Entries a route finds on a table build that its own table does not pin
/// and no pattern of the newest table finds there either, by table name.
const UNPINNED_UNCHECKED: &[(&str, &[&str])] = &[
    (
        "1.0 client (1269579)",
        &[
            "bases.cursor",
            "bases.entity_query",
            "bases.set_insert",
            "building.block_box",
            "recipe.limit_stop",
            "ui.scroll_focus",
            "ui.text_field",
            "ui.text_field_extent",
            "ui.text_field_text",
        ],
    ),
    (
        "1.0 server (1269579)",
        &[
            "bases.cursor",
            "bases.entity_query",
            "bases.set_insert",
            "building.block_box",
            "recipe.limit_stop",
        ],
    ),
];

/// On the executables `ENSH_SERVER_EXE` and `ENSH_CLIENT_EXE` name (each
/// skipped without it), every route of their side finds, with no table
/// behind it, the value their own table pins; an entry the table does not
/// pin takes the value the newest table's patterns find there, or is one of
/// [`UNPINNED_UNCHECKED`].
#[test]
#[cfg_attr(miri, ignore = "reading a game executable takes hours under Miri")]
fn every_route_finds_what_the_own_table_pins() {
    for (var, side) in [
        ("ENSH_SERVER_EXE", Side::Server),
        ("ENSH_CLIENT_EXE", Side::Client),
    ] {
        if let Ok(path) = std::env::var(var) {
            routes_hold_on(std::path::Path::new(&path), side);
        }
    }
}

/// As [`every_route_finds_what_the_own_table_pins`] on every executable the
/// opt-in `ENSH_EXES` lists (a path list, such as every kept build), each
/// a server when its file is `enshrouded_server.exe`; skipped without it.
#[test]
#[cfg_attr(miri, ignore = "reading a game executable takes hours under Miri")]
fn every_route_holds_on_every_kept_build() {
    let Some(list) = std::env::var_os("ENSH_EXES") else {
        return;
    };
    for path in std::env::split_paths(&list) {
        let server = path
            .file_name()
            .is_some_and(|name| name.eq_ignore_ascii_case("enshrouded_server.exe"));
        let side = if server { Side::Server } else { Side::Client };
        routes_hold_on(&path, side);
    }
}

/// Runs every route of `side` with no table behind it on the executable at
/// `path` and compares each with its own table, as the tests above say.
fn routes_hold_on(path: &std::path::Path, side: Side) {
    let var = path.display();
    let tables: Vec<Table> = embedded()
        .iter()
        .map(|(_, text)| Table::parse(text).unwrap())
        .collect();
    let routes = Routes::embedded().unwrap();
    {
        let bytes = std::fs::read(path).unwrap();
        let image = Image::parse(&bytes, Layout::File, None).unwrap();
        let reflection = Reflection::scan(&image);
        let own = exact(&tables, side, image.fingerprint().unwrap())
            .unwrap()
            .resolve();
        let text = image.range(image.section(".text").unwrap());
        let code = image.read(text.start, text.end - text.start).unwrap();
        let carried = crate::newest(&tables, side).unwrap().fall_back(
            code,
            text.start - image.base(),
            |at| image.u32(image.base() + at),
        );
        let unchecked: &[&str] = UNPINNED_UNCHECKED
            .iter()
            .find(|(name, _)| *name == own.name())
            .map_or(&[], |(_, ids)| ids);
        let wrong: Vec<String> = routes
            .find(&image, &reflection, side, &empty())
            .into_iter()
            .filter_map(|(id, routed)| {
                let (value, constant) = match routed {
                    Some(Routed::Symbol(v) | Routed::Reached(v)) => (Some(v), false),
                    Some(Routed::Constant(v)) => (Some(v), true),
                    None => (None, false),
                };
                let entry = |table: &crate::Resolved| {
                    if constant {
                        table.constant(id)
                    } else {
                        table.symbol(id)
                    }
                };
                let pinned = own.symbol(id).is_some() || own.constant(id).is_some();
                let want = if pinned {
                    entry(&own)
                } else if value.is_none() || unchecked.contains(&id) {
                    return None;
                } else {
                    entry(&carried)
                };
                (value != want).then(|| format!("{id}: route {routed:x?}, table {want:x?}"))
            })
            .collect();
        assert!(
            wrong.is_empty(),
            "{var} ({}), {} wrong:\n{}",
            own.name(),
            wrong.len(),
            wrong.join("\n")
        );
    }
}
