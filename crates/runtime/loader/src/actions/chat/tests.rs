use super::*;

#[test]
fn a_line_that_is_not_utf8_stays_within_the_limit_once_replaced() {
    let bytes = [0xffu8; 16];
    let span = [bytes.as_ptr().expose_provenance(), bytes.len()];
    // SAFETY: `span` views `bytes`, readable and unchanged for the call.
    let text = unsafe { read_text(&raw const span, 16) };
    assert_eq!(text, "\u{fffd}".repeat(5));
    assert!(text.len() <= 16);
}

#[test]
fn a_panic_in_the_part_for_mods_leaves_the_chat_mute_on() {
    let panicked = MODS_FUSE.run("a test", || panic!("a test"));
    assert_eq!(panicked, None::<()>);
    assert!(MODS_FUSE.is_off());
    assert!(!CHAT_FUSE.is_off(), "the mute and old frames keep working");
}

#[test]
fn a_panic_in_the_line_handling_leaves_the_post_linked_and_its_type_check_on() {
    let panicked = HANDLING_FUSE.run("a test", || panic!("a test"));
    assert_eq!(panicked, None::<()>);
    assert!(HANDLING_FUSE.is_off());
    assert!(!CHAT_FUSE.is_off(), "the post stays linked");
    assert_eq!(
        verdict(true, 2, false, false),
        Verdict::Drop,
        "a client's line of a type no client sends"
    );
    assert_eq!(verdict(true, EMOTE, false, false), Verdict::Pass);
}

#[test]
fn a_line_for_one_player_that_reached_others_names_no_loader_fault() {
    let turn = crate::engine::log::capture_turn();
    strayed(3);
    let lines = crate::engine::log::captured();
    drop(turn);
    let line = lines
        .iter()
        .map(|(_, _, line)| line.as_str())
        .find(|line| line.starts_with("chat lines to one player off: "));
    assert!(
        line.is_some_and(|line| !line.contains("vanilla") && !line.contains('(')),
        "{lines:?}"
    );
}

#[test]
fn a_line_to_one_player_needs_a_positive_byte_mask_and_overhead() {
    let one = OneLine {
        handle_slot: 0x7f,
        line_overhead: 0x17,
    };
    let (mask, overhead) = (
        Err(pinned::chat::HANDLE_SLOT.id()),
        Err(pinned::chat::LINE_OVERHEAD.id()),
    );
    assert_eq!(OneLine::from_rows(Some(0x7f), Some(0x17)), Ok(one));
    assert_eq!(OneLine::from_rows(None, Some(0x17)), mask, "no mask");
    assert_eq!(
        OneLine::from_rows(Some(0x7f), None),
        overhead,
        "no overhead"
    );
    assert_eq!(
        OneLine::from_rows(Some(0x80), Some(0x17)),
        mask,
        "a negative mask"
    );
    assert_eq!(
        OneLine::from_rows(Some(0x7f), Some(0xe9)),
        overhead,
        "a negative overhead"
    );
    assert_eq!(OneLine::from_rows(Some(0), Some(0x17)), mask, "a zero mask");
    assert_eq!(
        OneLine::from_rows(Some(0x7f), Some(0)),
        overhead,
        "a zero overhead"
    );
}

#[test]
fn a_build_without_the_chat_layout_names_the_first_entry_it_lacks() {
    let loader = crate::start::tests::bare_loader();
    assert_eq!(
        layout(&loader).err(),
        Some((pinned::chat::MARK_DELIVERED.id(), ABSENT))
    );
}

#[test]
fn a_build_without_the_chat_layout_lists_the_chat_lines_off() {
    let loader: &'static Loader = Box::leak(Box::new(crate::start::tests::bare_loader()));
    link(loader);
    assert!(CHAT_FUSE.is_off());
    let listed = crate::engine::diag::parts_off();
    let off = listed.iter().find(|p| p.part == CHAT_FUSE.feature());
    assert!(
        off.is_some_and(|p| p.reason.starts_with(pinned::chat::MARK_DELIVERED.id())),
        "{listed:?}"
    );
}

#[test]
fn a_line_for_one_player_that_reached_others_lists_those_lines_off() {
    let before = delivery().1;
    assert!(to_one_player());
    strayed(2);
    strayed(1);
    assert!(!to_one_player());
    assert_eq!(delivery().1 - before, 2, "every stray counts");
    let listed = crate::engine::diag::parts_off();
    let off = listed.iter().find(|p| p.part == "chat lines to one player");
    assert!(
        off.is_some_and(|p| p.reason.starts_with("a line for one player reached 2 ")),
        "{listed:?}"
    );
}

#[test]
fn a_line_counts_its_text_and_overhead_rounded_up_to_four() {
    assert_eq!(line_bytes(0, 0x17), 0x18);
    assert_eq!(line_bytes(1, 0x17), 0x18);
    assert_eq!(line_bytes(2, 0x17), 0x1c);
    assert_eq!(line_bytes(0x400, 0x17), 0x418);
    assert_eq!(line_bytes(usize::MAX, 0x17), u32::MAX);
}

#[test]
fn a_reply_keeps_its_lines_that_are_not_empty_up_to_a_bound() {
    let text = "said: hi\n\n  \n/enshwick ok";
    assert_eq!(
        replied(text).collect::<Vec<_>>(),
        ["said: hi", "/enshwick ok"]
    );
    assert_eq!(replied(&"x\n".repeat(40)).count(), LINES_PER_REPLY);
    assert_eq!(
        shown(&"x\n\n".repeat(40)).count(),
        40,
        "the reply to a typed command keeps every line"
    );
}

#[test]
fn replies_to_typed_commands_leave_half_the_waiting_lines_to_the_mods() {
    let help = "x\n".repeat(MAX_WAITING);
    assert_eq!(queue(To::All, replied("a mod's line"), MAX_WAITING), 0);
    let left_out: Vec<usize> = (1..=4)
        .map(|player| queue(To::Key(player), shown(&help), MAX_REPLY_WAITING))
        .collect();
    assert_eq!(
        OUTBOX.relock().len(),
        MAX_REPLY_WAITING,
        "replies stop at half"
    );
    assert_eq!(
        left_out,
        [
            MAX_WAITING - (MAX_REPLY_WAITING - 1),
            MAX_WAITING,
            MAX_WAITING,
            MAX_WAITING
        ],
        "every line left out is counted"
    );
    for _ in 0..MAX_WAITING {
        queue(To::All, replied("a mod's line"), MAX_WAITING);
    }
    assert_eq!(
        OUTBOX.relock().len(),
        MAX_WAITING,
        "the mods' lines still fill the rest"
    );
    forget();
}

#[test]
fn lines_are_cut_at_character_boundaries() {
    assert_eq!(cut("Сейф", 3), "С");
    assert_eq!(cut("safe", 10), "safe");
    assert_eq!(cut("safe", 0), "");
}

#[test]
fn a_player_s_line_of_any_chat_type_is_dropped_while_muted_or_an_old_frame() {
    for kind in [TYPED, EMOTE, 2, 4, 255] {
        assert_eq!(
            verdict(true, kind, true, false),
            Verdict::Drop,
            "kind {kind}"
        );
        assert_eq!(
            verdict(true, kind, false, true),
            Verdict::Drop,
            "kind {kind}"
        );
    }
}

#[test]
fn a_client_s_emote_is_heard_as_an_emote_and_only_its_typed_text_as_the_player_s() {
    assert_eq!(heard_as(true, TYPED), ChatLine::PLAYER);
    assert_eq!(heard_as(true, EMOTE), ChatLine::EMOTE);
    assert_eq!(heard_as(false, TYPED), ChatLine::NOTICE, "the server's own");
    assert_eq!(heard_as(false, EMOTE), ChatLine::EMOTE);
}

#[test]
fn a_player_s_typed_line_and_emote_keep_their_chat_type() {
    assert_eq!(verdict(true, TYPED, false, false), Verdict::Pass);
    assert_eq!(verdict(true, EMOTE, false, false), Verdict::Pass);
}

#[test]
fn a_player_s_line_of_another_chat_type_is_dropped_not_posted_as_empty_text() {
    for kind in [2, 3, 4, 255] {
        assert_eq!(
            verdict(true, kind, false, false),
            Verdict::Drop,
            "kind {kind}"
        );
    }
}

#[test]
fn lines_the_server_posts_itself_keep_their_chat_type() {
    for kind in [0, 1, 4, 255] {
        assert_eq!(verdict(false, kind, true, true), Verdict::Pass);
    }
}
