use super::*;

/// Where the fake code starts.
const TEXT: usize = 0x1000;
/// A module other than the game, from here to [`MODULE_END`].
const MODULE: usize = 0x9000;
const MODULE_END: usize = 0xA000;
/// Memory outside every module, where hook libraries put trampolines.
const TRAMPOLINE: usize = 0x5000;

fn module(at: usize) -> Option<String> {
    (MODULE..MODULE_END)
        .contains(&at)
        .then(|| "kiln-core.dll".to_owned())
}

/// `jmp rel32` at `from` to `to`.
fn near_jump(from: usize, to: usize) -> [u8; 5] {
    let delta = i32::try_from(to as isize - (from as isize + 5)).unwrap();
    let mut jump = [0xE9, 0, 0, 0, 0];
    jump[1..].copy_from_slice(&delta.to_le_bytes());
    jump
}

/// `jmp [rip+0]` with `to` after it, as hook libraries write it.
fn far_jump(to: usize) -> Vec<u8> {
    let mut jump = vec![0xFF, 0x25, 0, 0, 0, 0];
    jump.extend_from_slice(&(to as u64).to_le_bytes());
    jump
}

/// The 16 bytes a jump follower reads at `at`, from `code` placed at `start`.
fn memory(start: usize, code: &[u8]) -> impl Fn(usize) -> Option<Vec<u8>> + '_ {
    move |at| {
        let from = at.checked_sub(start)?;
        code.get(from..)
            .map(|rest| rest.iter().copied().take(16).collect())
    }
}

#[test]
fn code_as_shipped_is_taken_by_nobody() {
    let code = [0x90; 64];
    let taken = taken_in(&code, &code, TEXT, (TEXT + 8, 5), |_| None, module);
    assert_eq!(taken, None);
}

#[test]
fn a_jump_into_another_module_names_that_module() {
    let shipped = [0x90; 64];
    let mut live = shipped;
    live[8..13].copy_from_slice(&near_jump(TEXT + 8, MODULE + 0x40));
    let taken = taken_in(&shipped, &live, TEXT, (TEXT + 8, 5), |_| None, module);
    assert_eq!(taken, Some(Taken::By("kiln-core.dll".to_owned())));
}

#[test]
fn a_jump_through_a_trampoline_outside_every_module_names_where_it_ends() {
    let shipped = [0x90; 64];
    let mut live = shipped;
    live[8..13].copy_from_slice(&near_jump(TEXT + 8, TRAMPOLINE));
    let trampoline = far_jump(MODULE + 0x100);
    let read = memory(TRAMPOLINE, &trampoline);
    let taken = taken_in(&shipped, &live, TEXT, (TEXT + 8, 5), read, module);
    assert_eq!(taken, Some(Taken::By("kiln-core.dll".to_owned())));
}

#[test]
fn a_change_that_leads_into_no_module_is_taken_by_something_else() {
    let shipped = [0x90; 64];
    let mut live = shipped;
    live[8..13].copy_from_slice(&near_jump(TEXT + 8, TRAMPOLINE));
    let taken = taken_in(&shipped, &live, TEXT, (TEXT + 8, 5), |_| None, module);
    assert_eq!(
        taken,
        Some(Taken::Unknown),
        "a trampoline that does not read"
    );
    live[8..13].copy_from_slice(&[0xCC; 5]);
    let taken = taken_in(&shipped, &live, TEXT, (TEXT + 8, 5), |_| None, module);
    assert_eq!(taken, Some(Taken::Unknown), "bytes changed without a jump");
}

#[test]
fn a_change_beside_the_hooked_bytes_is_not_their_holder() {
    let shipped = [0x90; 64];
    let mut live = shipped;
    live[20..25].copy_from_slice(&near_jump(TEXT + 20, MODULE));
    let taken = taken_in(&shipped, &live, TEXT, (TEXT + 8, 5), |_| None, module);
    assert_eq!(taken, None);
}

#[test]
fn bytes_outside_the_shipped_code_are_not_judged() {
    let shipped = [0x90; 64];
    let live = [0xCC; 64];
    for span in [(0x10, 5), (TEXT + 62, 5), (usize::MAX, 5)] {
        let taken = taken_in(&shipped, &live, TEXT, span, |_| None, module);
        assert_eq!(taken, None, "{span:x?}");
    }
}

#[test]
fn a_holder_reads_as_who_took_the_code() {
    assert_eq!(
        Taken::By("kiln-core.dll".to_owned()).to_string(),
        "taken by kiln-core.dll"
    );
    assert_eq!(
        Taken::Unknown.to_string(),
        "taken by something other than Enshwick"
    );
}

#[test]
fn a_module_is_named_by_its_file_and_the_game_s_own_code_is_none() {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;

    let kernel32: Vec<u16> = "kernel32.dll\0".encode_utf16().collect();
    // SAFETY: `kernel32` is a NUL-terminated wide string; kernel32 is
    // loaded in every process.
    let system = unsafe { GetModuleHandleW(kernel32.as_ptr()) }.addr();
    let name = module_of(system).expect("a system DLL");
    assert!(name.eq_ignore_ascii_case("kernel32.dll"), "{name}");
    let own =
        (a_module_is_named_by_its_file_and_the_game_s_own_code_is_none as fn() as *const ()).addr();
    assert_eq!(module_of(own), None, "the executable's own code");
    assert_eq!(module_of(0x10), None, "no module");
}
