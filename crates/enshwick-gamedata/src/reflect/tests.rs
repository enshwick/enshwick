use super::*;

/// A minimal PE32+ file: headers, then `.text` and `.rdata`, each laid out
/// at the same offset in the file as its RVA, so tests can write data at
/// virtual addresses directly.
struct Fake {
    bytes: Vec<u8>,
}

const BASE: u64 = 0x1_4000_0000;
const TEXT: u64 = 0x1000;
const RDATA: u64 = 0x2000;
const SIZE: u64 = 0x1000;

impl Fake {
    fn new() -> Self {
        let mut bytes = vec![0u8; usize::try_from(RDATA + SIZE).unwrap()];
        bytes[..2].copy_from_slice(b"MZ");
        bytes[0x3c..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        bytes[0x80..0x84].copy_from_slice(b"PE\0\0");
        bytes[0x86..0x88].copy_from_slice(&2u16.to_le_bytes());
        bytes[0x94..0x96].copy_from_slice(&0xf0u16.to_le_bytes());
        let optional = 0x98;
        bytes[optional..optional + 2].copy_from_slice(&0x20bu16.to_le_bytes());
        bytes[optional + 24..optional + 32].copy_from_slice(&BASE.to_le_bytes());
        let headers = optional + 0xf0;
        for (i, (name, rva)) in [(&b".text\0\0\0"[..], TEXT), (&b".rdata\0\0"[..], RDATA)]
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

    fn put(&mut self, va: u64, data: &[u8]) {
        let at = usize::try_from(va - BASE).unwrap();
        self.bytes[at..at + data.len()].copy_from_slice(data);
    }

    fn put_u64(&mut self, va: u64, value: u64) {
        self.put(va, &value.to_le_bytes());
    }

    /// Writes `text` NUL-terminated at `at` and a slice to it at `slot`.
    fn slice(&mut self, slot: u64, at: u64, text: &str) {
        self.put(at, text.as_bytes());
        self.put(at + text.len() as u64, &[0]);
        self.put_u64(slot, at);
        self.put_u64(slot + 8, text.len() as u64);
    }
}

fn rdata(offset: u64) -> u64 {
    BASE + RDATA + offset
}

#[test]
fn fnv1a32_matches_the_reference_values() {
    assert_eq!(fnv1a32(b""), 0x811c_9dc5);
    assert_eq!(fnv1a32(b"a"), 0xe40c_292c);
    assert_eq!(fnv1a32(b"foobar"), 0xbf9c_f968);
}

#[test]
fn bytes_that_are_not_pe_are_refused() {
    assert_eq!(
        Image::parse(b"MZ", Layout::File, None).unwrap_err(),
        ImageError::Truncated
    );
    assert_eq!(
        Image::parse(b"ELF", Layout::File, None).unwrap_err(),
        ImageError::NotPe
    );
}

#[test]
fn sections_and_addresses_resolve() {
    let mut fake = Fake::new();
    fake.put(rdata(0x10), b"abcd");
    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    assert_eq!(image.base(), BASE);
    assert_eq!(image.section(".rdata").unwrap().rva(), RDATA);
    assert_eq!(image.read(rdata(0x10), 4), Some(&b"abcd"[..]));
    assert_eq!(image.read(rdata(SIZE - 2), 4), None);
    assert!(image.fingerprint().is_none());
}

#[test]
fn an_image_at_the_top_of_the_address_space_reads_nothing_and_does_not_panic() {
    let mut fake = Fake::new();
    // A debug directory of 0xffff_ffff bytes at `.rdata`.
    let optional = 0x98;
    let debug = optional + 112 + 6 * 8;
    let rva = u32::try_from(RDATA).unwrap();
    fake.bytes[debug..debug + 4].copy_from_slice(&rva.to_le_bytes());
    fake.bytes[debug + 4..debug + 8].copy_from_slice(&u32::MAX.to_le_bytes());
    let top = u64::MAX - 0x100;
    let image = Image::parse(&fake.bytes, Layout::File, Some(top)).unwrap();
    assert!(image.fingerprint().is_none());
    assert_eq!(image.read(top, u64::MAX), None);
    let reflection = Reflection::scan(&image);
    assert_eq!(reflection.type_names().count(), 0);
}

#[test]
fn a_descriptor_and_its_fields_are_found() {
    let mut fake = Fake::new();
    let descriptor = rdata(0x100);
    let name = "keen::ecs::TriggerShape";
    fake.slice(
        descriptor + DESCRIPTOR_SHORT_NAME,
        rdata(0x800),
        "TriggerShape",
    );
    fake.slice(descriptor + DESCRIPTOR_FULL_NAME, rdata(0x820), name);
    fake.put(descriptor + DESCRIPTOR_SIZE_ALIGN, &0x10u32.to_le_bytes());
    fake.put(descriptor + DESCRIPTOR_SIZE_ALIGN + 4, &4u16.to_le_bytes());
    fake.put(descriptor + DESCRIPTOR_FIELD_COUNT, &2u32.to_le_bytes());
    fake.put(
        descriptor + DESCRIPTOR_HASH,
        &fnv1a32(name.as_bytes()).to_le_bytes(),
    );
    let table = rdata(0x400);
    fake.put_u64(descriptor + DESCRIPTOR_FIELDS, table);
    fake.slice(table, rdata(0x840), "rangeX");
    fake.put_u64(table + FIELD_OFFSET, 4);
    fake.slice(table + FIELD_RECORD, rdata(0x850), "rangeY");
    fake.put_u64(table + FIELD_RECORD + FIELD_OFFSET, 8);

    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let info = reflection.type_info(name).unwrap();
    assert_eq!(
        (info.size(), info.align(), info.fields().len()),
        (0x10, 4, 2)
    );
    assert_eq!(reflection.field_offset(name, "rangeY"), Ok(8));
    assert_eq!(
        reflection.field_offset(name, "rangeZ"),
        Err(LookupError::Missing(format!("{name}.rangeZ")))
    );
}

#[test]
fn an_unreadable_field_record_drops_that_field_only() {
    let mut fake = Fake::new();
    let descriptor = rdata(0x100);
    let name = "keen::ecs::TriggerShape";
    fake.slice(
        descriptor + DESCRIPTOR_SHORT_NAME,
        rdata(0x800),
        "TriggerShape",
    );
    fake.slice(descriptor + DESCRIPTOR_FULL_NAME, rdata(0x820), name);
    fake.put(descriptor + DESCRIPTOR_SIZE_ALIGN, &0x10u32.to_le_bytes());
    fake.put(descriptor + DESCRIPTOR_SIZE_ALIGN + 4, &4u16.to_le_bytes());
    fake.put(descriptor + DESCRIPTOR_FIELD_COUNT, &3u32.to_le_bytes());
    fake.put(
        descriptor + DESCRIPTOR_HASH,
        &fnv1a32(name.as_bytes()).to_le_bytes(),
    );
    let table = rdata(0x400);
    fake.put_u64(descriptor + DESCRIPTOR_FIELDS, table);
    fake.slice(table, rdata(0x840), "rangeX");
    fake.put_u64(table + FIELD_OFFSET, 4);
    // The second record's name points outside the image.
    fake.put_u64(table + FIELD_RECORD, 0x10);
    fake.put_u64(table + FIELD_RECORD + 8, 6);
    fake.slice(table + 2 * FIELD_RECORD, rdata(0x850), "rangeZ");
    fake.put_u64(table + 2 * FIELD_RECORD + FIELD_OFFSET, 12);

    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let info = reflection.type_info(name).unwrap();
    assert_eq!(info.fields().len(), 2);
    assert_eq!(reflection.field_offset(name, "rangeX"), Ok(4));
    assert_eq!(reflection.field_offset(name, "rangeZ"), Ok(12));
}

/// Writes a descriptor of `kind` named `name` at `at`, deriving from
/// or holding the descriptor at `inner`, with its strings at `strings`.
fn describe(fake: &mut Fake, at: u64, strings: u64, name: &str, kind: u8, inner: Option<u64>) {
    fake.slice(at + DESCRIPTOR_SHORT_NAME, strings, name);
    fake.slice(at + DESCRIPTOR_FULL_NAME, strings + 0x40, name);
    fake.put(at + DESCRIPTOR_KIND, &[kind]);
    fake.put(
        at + DESCRIPTOR_HASH,
        &fnv1a32(name.as_bytes()).to_le_bytes(),
    );
    if let Some(inner) = inner {
        fake.put_u64(at + DESCRIPTOR_INNER, inner + DESCRIPTOR_SHORT_NAME);
    }
}

#[test]
fn a_struct_names_its_base_and_types_derived_from_a_root_are_listed() {
    let mut fake = Fake::new();
    let at = |i: u64| rdata(0x100 * i);
    let strings = |i: u64| rdata(0x800 + 0x80 * i);
    let types = [
        ("keen::ecs::Event", KIND_STRUCT, None),
        ("keen::ecs::GameEvent", KIND_STRUCT, Some(at(1))),
        ("keen::ecs::HitEvent", KIND_STRUCT, Some(at(2))),
        (
            "keen::StaticArray<keen::ecs::HitEvent,10>",
            0x13,
            Some(at(3)),
        ),
        ("keen::ecs::Health", KIND_STRUCT, None),
        ("keen::Loop", KIND_STRUCT, Some(at(7))),
        ("keen::Pool", KIND_STRUCT, Some(at(6))),
    ];
    for (i, (name, kind, inner)) in (1..).zip(types) {
        describe(&mut fake, at(i), strings(i), name, kind, inner);
    }

    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let hit = reflection.type_info("keen::ecs::HitEvent").unwrap();
    assert_eq!(hit.base(), Some("keen::ecs::GameEvent"));
    let array = reflection
        .type_info("keen::StaticArray<keen::ecs::HitEvent,10>")
        .unwrap();
    assert_eq!(
        array.base(),
        None,
        "an array holds its type, it does not derive"
    );
    assert_eq!(
        reflection.type_info("keen::ecs::Event").unwrap().base(),
        None
    );
    let mut derived: Vec<&str> = reflection
        .derived_from("keen::ecs::Event")
        .map(TypeInfo::name)
        .collect();
    derived.sort_unstable();
    assert_eq!(derived, ["keen::ecs::GameEvent", "keen::ecs::HitEvent"]);
    assert_eq!(reflection.derived_from("keen::ecs::Missing").count(), 0);
    assert_eq!(
        reflection.derived_from("keen::Root").count(),
        0,
        "a loop of bases ends"
    );
}

#[test]
fn kinds_inner_types_and_array_lengths_are_read() {
    let mut fake = Fake::new();
    let at = |i: u64| rdata(0x100 * i);
    let strings = |i: u64| rdata(0x800 + 0x80 * i);
    let types = [
        ("keen::uint32", 0x06, None),
        ("keen::ItemId", 0x11, Some(at(1))),
        ("keen::StaticArray<keen::uint32,4>", 0x13, Some(at(1))),
        ("keen::Thing", 0x7f, None),
    ];
    for (i, (name, kind, inner)) in (1..).zip(types) {
        describe(&mut fake, at(i), strings(i), name, kind, inner);
    }
    fake.put(at(3) + DESCRIPTOR_FIELD_COUNT, &4u32.to_le_bytes());

    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let primitive = reflection.type_info("keen::uint32").unwrap();
    assert_eq!((primitive.kind(), primitive.inner()), (Kind::Uint32, None));
    let alias = reflection.type_info("keen::ItemId").unwrap();
    assert_eq!(alias.kind(), Kind::Typedef);
    assert_eq!(alias.inner(), Some("keen::uint32"));
    assert_eq!((alias.base(), alias.element_count()), (None, None));
    let array = reflection
        .type_info("keen::StaticArray<keen::uint32,4>")
        .unwrap();
    assert_eq!(array.kind(), Kind::StaticArray);
    assert_eq!(array.element_count(), Some(4));
    assert!(
        array.fields().is_empty(),
        "an array's count is no field count"
    );
    assert_eq!(
        reflection.type_info("keen::Thing").unwrap().kind(),
        Kind::Other(0x7f)
    );
}

#[test]
fn only_the_engine_s_kind_bytes_have_names() {
    for byte in 0..=u8::MAX {
        let kind = Kind::from_byte(byte);
        let named = !matches!(kind, Kind::Other(_));
        assert_eq!(
            named,
            (0x01..=0x1d).contains(&byte) && byte != 0x0b,
            "{byte:#x}"
        );
    }
}

/// Writes an enum descriptor named `name` at `descriptor` with the
/// constants `values`, its strings from `strings` on.
fn put_enum(fake: &mut Fake, descriptor: u64, strings: u64, name: &str, values: &[(&str, u64)]) {
    fake.slice(descriptor + DESCRIPTOR_SHORT_NAME, strings, "Slot");
    fake.slice(descriptor + DESCRIPTOR_FULL_NAME, strings + 0x10, name);
    fake.put(descriptor + DESCRIPTOR_SIZE_ALIGN, &1u32.to_le_bytes());
    fake.put(descriptor + DESCRIPTOR_SIZE_ALIGN + 4, &1u16.to_le_bytes());
    let count = u32::try_from(values.len()).unwrap();
    fake.put(descriptor + DESCRIPTOR_FIELD_COUNT, &count.to_le_bytes());
    fake.put(descriptor + DESCRIPTOR_KIND, &[KIND_ENUM]);
    fake.put(
        descriptor + DESCRIPTOR_HASH,
        &fnv1a32(name.as_bytes()).to_le_bytes(),
    );
    let table = strings + 0x100;
    fake.put_u64(descriptor + DESCRIPTOR_ENUM_VALUES, table);
    for (i, &(constant, value)) in values.iter().enumerate() {
        let record = table + i as u64 * ENUM_RECORD;
        fake.slice(record, strings + 0x40 + i as u64 * 0x20, constant);
        fake.put_u64(record + ENUM_VALUE, value);
    }
}

#[test]
fn an_enum_s_constants_are_found_by_name() {
    let mut fake = Fake::new();
    let name = "keen::EquipmentSlotType";
    put_enum(
        &mut fake,
        rdata(0x100),
        rdata(0x800),
        name,
        &[("Invalid", 0), ("Roof", 11), ("Overgrowth", 12)],
    );

    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let info = reflection.type_info(name).unwrap();
    assert!(info.fields().is_empty());
    let names: Vec<&str> = info.enum_values().iter().map(EnumValue::name).collect();
    assert_eq!(names, ["Invalid", "Roof", "Overgrowth"]);
    assert_eq!(reflection.enum_value(name, "Roof"), Ok(11));
    assert_eq!(
        reflection.enum_value(name, "Terrain"),
        Err(LookupError::Missing(format!("{name}::Terrain")))
    );
    assert_eq!(
        reflection.enum_value("keen::NoSuchEnum", "Roof"),
        Err(LookupError::Missing("keen::NoSuchEnum".into()))
    );
}

#[test]
fn a_struct_reads_no_enum_constants_and_an_enum_no_fields() {
    let mut fake = Fake::new();
    let name = "keen::EquipmentSlotType";
    put_enum(&mut fake, rdata(0x100), rdata(0x800), name, &[("Roof", 11)]);
    // The same descriptor without the enum kind is a struct whose field
    // table is null.
    fake.put(rdata(0x100) + DESCRIPTOR_KIND, &[0]);

    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let info = reflection.type_info(name).unwrap();
    assert!(info.enum_values().is_empty());
    assert!(info.fields().is_empty());
    assert_eq!(
        reflection.enum_value(name, "Roof"),
        Err(LookupError::Missing(format!("{name}::Roof")))
    );
}

#[test]
fn an_unreadable_enum_record_drops_that_constant_only() {
    let mut fake = Fake::new();
    let name = "keen::EquipmentSlotType";
    put_enum(
        &mut fake,
        rdata(0x100),
        rdata(0x800),
        name,
        &[("Invalid", 0), ("Roof", 11), ("Overgrowth", 12)],
    );
    // The second record's name points outside the image.
    let table = rdata(0x900);
    fake.put_u64(table + ENUM_RECORD, 0x10);

    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let info = reflection.type_info(name).unwrap();
    assert_eq!(info.enum_values().len(), 2);
    assert_eq!(reflection.enum_value(name, "Overgrowth"), Ok(12));
}

#[test]
fn a_system_record_is_found_and_lookups_never_guess() {
    let mut fake = Fake::new();
    let entry = BASE + TEXT + 0x40;
    for record in [rdata(0x100), rdata(0x200)] {
        fake.slice(record, rdata(0x800), "add_buildzones_server");
        fake.put_u64(record + SYSTEM_ENTRY, entry);
        fake.put_u64(record + SYSTEM_LISTS[2], rdata(0x600));
        fake.put_u64(record + SYSTEM_LISTS[2] + 8, 1);
    }
    fake.slice(rdata(0x600), rdata(0x880), "TriggerShape");

    let image = Image::parse(&fake.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    assert_eq!(
        reflection.system("add_buildzones_server"),
        Err(LookupError::Ambiguous("add_buildzones_server".into()))
    );

    let mut single = Fake::new();
    single.bytes.clone_from(&fake.bytes);
    single.put(rdata(0x200), &[0; 0x80]);
    let image = Image::parse(&single.bytes, Layout::File, None).unwrap();
    let reflection = Reflection::scan(&image);
    let system = reflection.system("add_buildzones_server").unwrap();
    assert_eq!(system.entry(), entry);
    assert_eq!(system.writes(), ["TriggerShape"]);
    assert!(system.triggers().is_empty());
}
