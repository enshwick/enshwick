//! Shapes matched on decoded code, without an image: what each operand
//! form matches and what a run takes.

use iced_x86::{Decoder, DecoderOptions, Instruction};

use crate::routes::shape::{self, Step};

/// `mov r8d, 0x1b8; lea rdx, [rbp-0x60]; mov rcx, r13; call +0; mov ecx,
/// [rip+0x10]; call -0x0a; mov eax, [rcx+r10*4+0x10]`, at 0x1000: the calls
/// go to 0x1012 and 0x1013.
const CODE: [u8; 34] = [
    0x41, 0xb8, 0xb8, 0x01, 0x00, 0x00, 0x48, 0x8d, 0x55, 0xa0, 0x49, 0x8b, 0xcd, 0xe8, 0x00, 0x00,
    0x00, 0x00, 0x8b, 0x0d, 0x10, 0x00, 0x00, 0x00, 0xe8, 0xf6, 0xff, 0xff, 0xff, 0x42, 0x8b, 0x44,
    0x91, 0x10,
];

fn steps(text: &[&str]) -> Vec<Step> {
    let names = shape::ShapeNames::new();
    text.iter()
        .map(|s| Step::parse(s, &names).unwrap())
        .collect()
}

fn first(text: &[&str], entry: &impl Fn(&str) -> Option<u64>) -> Option<shape::Match> {
    first_in(&CODE, text, entry)
}

fn first_in(
    bytes: &[u8],
    text: &[&str],
    entry: &impl Fn(&str) -> Option<u64>,
) -> Option<shape::Match> {
    let steps = steps(text);
    let mut decoder = Decoder::with_ip(64, bytes, 0x1000, DecoderOptions::NONE);
    let code: Vec<Instruction> =
        core::iter::from_fn(|| decoder.can_decode().then(|| decoder.decode())).collect();
    shape::matches(&steps, &code, entry).next()
}

/// `mov [rcx+0x8],eax; mov [rcx+0x4],eax; mov [rcx],al`, then the same
/// stores with the middle one through `rdx`.
const ONE_BASE: [u8; 8] = [0x89, 0x41, 0x08, 0x89, 0x41, 0x04, 0x88, 0x01];
const TWO_BASES: [u8; 8] = [0x89, 0x41, 0x08, 0x89, 0x42, 0x04, 0x88, 0x01];

#[test]
fn a_bound_register_is_the_same_register_at_every_use_in_the_run() {
    let none = |_: &str| None;
    let stores = ["mov [?1+*],eax", "mov [?1+?],eax", "mov [?1],al"];
    let found = first_in(&ONE_BASE, &stores, &none).unwrap();
    assert_eq!(
        (found.start, found.end, found.taken),
        (0x1000, 0x1008, Some(8))
    );
    assert!(first_in(&TWO_BASES, &stores, &none).is_none());
    let loose = ["mov [?+*],eax", "mov [?+?],eax", "mov [?],al"];
    assert!(
        first_in(&TWO_BASES, &loose, &none).is_some(),
        "`?` binds nothing"
    );
    let two = ["mov [?1+*],eax", "mov [?2+?],eax", "mov [?1],al"];
    assert!(
        first_in(&TWO_BASES, &two, &none).is_some(),
        "two holes, two registers"
    );
    assert!(
        first_in(&ONE_BASE, &two, &none).is_some(),
        "two holes may hold one register"
    );
}

#[test]
fn a_bound_register_binds_across_operands_and_widths_but_never_rip() {
    let none = |_: &str| None;
    // mov rbx,rcx; mov ebx,0x1; mov eax,[rbx+0x10]
    let code = [0x48, 0x8b, 0xd9, 0xbb, 1, 0, 0, 0, 0x8b, 0x43, 0x10];
    let found = first_in(&code, &["mov ?1,rcx", "mov ?1,?i", "mov eax,[?1+*]"], &none);
    assert_eq!(found.map(|m| m.taken), Some(Some(0x10)));
    assert!(first_in(&code, &["mov ?1,rcx", "mov eax,[?1+*]"], &none).is_none());
    assert!(first_in(&code, &["mov rbx,?1", "mov eax,[?1+?]"], &none).is_none());
    assert!(
        first(&["mov ecx,[?1+?]"], &none).is_none(),
        "RIP is no bound base"
    );
}

#[test]
fn a_shape_names_a_constant_entry_as_a_displacement_or_an_immediate() {
    let is = |want: u64| move |id: &str| (id == "c.offset").then_some(want);
    assert!(first(&["mov eax,[rcx+r10*4+{c.offset}]"], &is(0x10)).is_some());
    assert!(first(&["mov eax,[rcx+r10*4+{c.offset}]"], &is(0x18)).is_none());
    assert!(
        first(&["lea rdx,[rbp+{c.offset}]"], &is(0x60)).is_none(),
        "the sign counts"
    );
    assert!(first(&["mov r8d,{c.offset}"], &is(0x1b8)).is_some());
    assert!(first(&["mov eax,[?+{c.missing}]"], &is(0x10)).is_none());
}

#[test]
fn a_hole_out_of_range_or_a_subtracted_entry_does_not_parse() {
    let names = shape::ShapeNames::new();
    for bad in [
        "mov ?0,rcx",
        "mov ?10,rcx",
        "mov eax,[?0+?]",
        "mov eax,[rcx-{c.offset}]",
    ] {
        assert!(Step::parse(bad, &names).is_err(), "{bad}");
    }
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

#[test]
fn a_memory_operand_through_any_base_matches_every_register_but_rip() {
    let none = |_: &str| None;
    let any = first(&["lea rdx,[?-*]"], &none).unwrap();
    assert_eq!((any.start, any.taken), (0x1006, Some(0x60)));
    assert!(first(&["mov eax,[?+r10*4+0x10]"], &none).is_some());
    assert!(first(&["mov ecx,[?+?]"], &none).is_none());
}

#[test]
fn a_memory_operand_off_the_stack_matches_no_stack_frame_or_rip_base() {
    let text = |_: &str| Some(0x3a0);
    let read = ["movups xmm0,[?o+{text}]"];
    // movups xmm0,[<base>+0x3a0], encoded per base.
    let through = |bytes: &[u8]| first_in(bytes, &read, &text).is_some();
    assert!(through(&[0x0f, 0x10, 0x87, 0xa0, 3, 0, 0]), "rdi");
    assert!(
        through(&[0x41, 0x0f, 0x10, 0x84, 0x24, 0xa0, 3, 0, 0]),
        "r12"
    );
    assert!(through(&[0x41, 0x0f, 0x10, 0x85, 0xa0, 3, 0, 0]), "r13");
    assert!(!through(&[0x0f, 0x10, 0x84, 0x24, 0xa0, 3, 0, 0]), "rsp");
    assert!(!through(&[0x0f, 0x10, 0x85, 0xa0, 3, 0, 0]), "rbp");
    assert!(
        !through(&[0x67, 0x0f, 0x10, 0x84, 0x24, 0xa0, 3, 0, 0]),
        "esp"
    );
    let none = |_: &str| None;
    let rip = [0x0f, 0x10, 0x05, 0xa0, 3, 0, 0];
    assert!(first_in(&rip, &["movups xmm0,[?+?]"], &none).is_none());
    assert!(first_in(&rip, &["movups xmm0,[?o+?]"], &none).is_none());
    assert!(
        first_in(
            &[0x0f, 0x10, 0x85, 0xa0, 3, 0, 0],
            &["movups xmm0,[?+?]"],
            &none
        )
        .is_some(),
        "`?` takes a frame base"
    );
}
