use enshwick_core::abi::{ModHandle, Status, Str};

use super::*;
use crate::engine::profile::Feature;

/// A domain error of the tests, of each policy.
#[derive(Debug, thiserror::Error)]
enum LampError {
    #[error("no lamp is lit")]
    Dark,
    #[error("lamp {0} burns another mod's oil")]
    Borrowed(u32),
}

impl Refusal for LampError {
    fn status(&self) -> Status {
        match self {
            Self::Dark => Status::MISSING,
            Self::Borrowed(_) => Status::REFUSED,
        }
    }

    fn policy(&self) -> Policy {
        match self {
            Self::Dark => Policy::Quiet,
            Self::Borrowed(_) => Policy::Warn,
        }
    }
}

entries! {
    fn lamp(lit: u32) -> Status {
        if lit == 0 {
            return Err(LampError::Dark.into());
        }
        if lit > 9 {
            return Err(LampError::Borrowed(lit).into());
        }
        Ok(())
    }

    fn lamp_name(name as Text, out as Out<u32>) -> Status {
        out.put(u32::try_from(name.len()).unwrap_or(u32::MAX))
    }

    fn lamp_level() -> u32 {
        Ok(7)
    }
}

entries! {
    fn glow(broken: u32) -> Status {
        assert!(broken == 0, "the wick snapped");
        Ok(())
    }

    fn glow_beside() -> Status {
        Ok(())
    }

    fn glow_into(out as Out<u32>, broken: u32) -> Status {
        assert!(broken == 0, "the counted wick snapped");
        out.put(1)
    }

    fn snuff() -> Status {
        checked(smoke())
    }
}

/// Panics with a [`Smoke`] payload.
fn smoke() -> Status {
    std::panic::panic_any(Smoke)
}

/// A panic payload whose drop panics again.
struct Smoke;

impl Drop for Smoke {
    fn drop(&mut self) {
        assert!(std::thread::panicking(), "the smoke caught fire");
    }
}

entries! {
    fn wick() -> u32 {
        crate::engine::profile::call(ModIndex::new(3), Feature::Calls, || -> u32 {
            panic!("the mod's wick snapped")
        });
        Ok(1)
    }
}

entries! {
    fn oil(_me as Mod) -> Status {
        Ok(())
    }

    fn oil_total([cans, count] as UnalignedList<u64>, total as Out<u64>) -> Status {
        total.put(cans.iter().sum())
    }

    fn oil_fill([out, capacity] as Buffer<u32, u64>) -> Status {
        out.fill(1);
        Ok(())
    }
}

#[test]
#[expect(
    clippy::cast_ptr_alignment,
    reason = "the entry gets an unaligned list and out on purpose"
)]
fn an_unaligned_list_and_out_are_read_and_written_where_the_mod_put_them() {
    let mut bytes = [0u8; 32];
    for (i, can) in [3u64, 4].iter().enumerate() {
        bytes[1 + i * 8..9 + i * 8].copy_from_slice(&can.to_ne_bytes());
    }
    let cans = bytes.as_ptr().wrapping_add(1).cast::<u64>();
    let total = bytes.as_mut_ptr().wrapping_add(19).cast::<u64>();
    assert_eq!(oil_total(cans, 2, total), Status::OK);
    assert_eq!(bytes[19..27], 7u64.to_ne_bytes());
    assert_eq!(oil_total(core::ptr::null(), 1, total), Status::BAD_ARGS);
}

#[test]
fn a_buffer_past_the_address_space_is_refused_before_the_body_runs() {
    let mut cans = [0u32; 2];
    assert_eq!(oil_fill(cans.as_mut_ptr(), u64::MAX), Status::BAD_ARGS);
    assert_eq!(oil_fill(cans.as_mut_ptr(), 2), Status::OK);
    assert_eq!(cans, [1, 1]);
}

#[test]
fn a_quiet_refusal_answers_its_status_and_logs_nothing() {
    let _turn = crate::engine::log::capture_turn();
    assert_eq!(lamp(0), Status::MISSING);
    assert!(crate::engine::log::captured().is_empty());
}

#[test]
fn a_refusal_to_warn_of_answers_its_status_and_logs_its_reason() {
    let _turn = crate::engine::log::capture_turn();
    assert_eq!(lamp(12), Status::REFUSED);
    let logged = crate::engine::log::captured();
    assert!(
        logged
            .iter()
            .any(|(_, _, line)| line == "lamp: lamp 12 burns another mod's oil"),
        "{logged:?}"
    );
}

#[test]
fn a_body_s_answer_becomes_the_entry_s_return_value() {
    assert_eq!(lamp(3), Status::OK);
    assert_eq!(lamp_level(), 7);
}

#[test]
fn a_text_and_an_out_are_read_and_written_and_a_null_out_is_refused() {
    let mut len = 0u32;
    assert_eq!(lamp_name(Str::new("wick"), &raw mut len), Status::OK);
    assert_eq!(len, 4);
    assert_eq!(
        lamp_name(Str::new("wick"), core::ptr::null_mut()),
        Status::BAD_ARGS
    );
}

#[test]
fn a_panicking_entry_turns_itself_off_and_later_calls_answer_as_the_panic_did() {
    assert_eq!(glow(1), Status::REFUSED);
    assert_eq!(glow(0), Status::REFUSED);
    let off = crate::engine::diag::parts_off();
    let glow_off = off.iter().find(|part| part.part == "glow");
    assert_eq!(
        glow_off.map(|part| part.reason.as_str()),
        Some("the loader panicked in glow: the wick snapped")
    );
}

#[test]
fn a_panicking_entry_leaves_the_other_entries_of_its_file_on() {
    assert_eq!(glow(2), Status::REFUSED);
    assert_eq!(glow_beside(), Status::OK);
}

#[test]
fn an_entry_that_is_off_still_checks_its_arguments_first() {
    let mut lit = 0u32;
    assert_eq!(glow_into(&raw mut lit, 1), Status::REFUSED);
    assert_eq!(glow_into(core::ptr::null_mut(), 0), Status::BAD_ARGS);
    assert_eq!(glow_into(&raw mut lit, 0), Status::REFUSED);
    assert_eq!(lit, 0);
}

#[test]
fn a_panic_whose_payload_panics_as_it_drops_never_unwinds_into_the_mod() {
    let answered = std::panic::catch_unwind(|| snuff());
    assert_eq!(answered.ok(), Some(Status::REFUSED));
}

#[test]
fn a_panic_inside_a_mod_s_callback_blames_the_mod_and_leaves_the_entry_on() {
    let before = crate::engine::profile::marker();
    assert_eq!(wick(), 0);
    let off = crate::engine::diag::parts_off();
    assert!(off.iter().all(|part| part.part != "wick"), "{off:?}");
    assert_eq!(
        crate::engine::profile::marker(),
        before,
        "the outer marker is back"
    );
}

#[test]
fn a_mod_handle_without_a_loader_is_refused_as_a_bad_argument() {
    assert_eq!(oil(ModHandle(1)), Status::BAD_ARGS);
}
