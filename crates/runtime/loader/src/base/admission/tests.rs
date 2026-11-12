use super::*;

const SERVER: &str = "you.trading-post@0.3.0,zones@0.1.0";

fn rules() -> Rules {
    Rules {
        wait: Duration::from_secs(30),
        grace: Duration::from_secs(10),
    }
}

fn need(id: &str, version: &str, yours: Option<&str>) -> Need {
    Need {
        id: id.to_owned(),
        version: version.to_owned(),
        yours: yours.map(str::to_owned),
    }
}

fn said(mods: &str) -> Hello {
    Hello {
        mods: mods.to_owned(),
        complete: true,
        nonce: 0,
        language: None,
    }
}

fn note(key: u64, hello: bool, needs: Vec<Need>) -> Step {
    Step::Note { key, hello, needs }
}

fn all_lacking() -> Vec<Need> {
    vec![
        need("you.trading-post", "0.3.0", None),
        need("zones", "0.1.0", None),
    ]
}

fn at(start: Instant, seconds: u64) -> Instant {
    start + Duration::from_secs(seconds)
}

#[test]
fn a_game_that_never_says_hello_is_told_then_kicked() {
    let mut gate = Gate::default();
    let start = Instant::now();
    assert_eq!(gate.joined(7, SERVER, start, rules()), []);
    assert!(gate.busy());
    assert_eq!(gate.due(SERVER, at(start, 29), rules()), []);
    assert_eq!(
        gate.due(SERVER, at(start, 30), rules()),
        [note(7, false, all_lacking())]
    );
    assert_eq!(gate.due(SERVER, at(start, 30), rules()), [], "one note");
    assert_eq!(
        gate.due(SERVER, at(start, 40), rules()),
        [Step::Kick { key: 7 }]
    );
    assert_eq!(
        gate.due(SERVER, at(start, 41), rules()),
        [],
        "one kick at a time"
    );
}

#[test]
fn a_kick_that_did_not_remove_the_player_is_tried_again() {
    let mut gate = Gate::default();
    let start = Instant::now();
    gate.joined(7, SERVER, start, rules());
    gate.due(SERVER, at(start, 30), rules());
    assert_eq!(
        gate.due(SERVER, at(start, 40), rules()),
        [Step::Kick { key: 7 }]
    );
    assert_eq!(gate.due(SERVER, at(start, 49), rules()), []);
    assert_eq!(
        gate.due(SERVER, at(start, 50), rules()),
        [Step::Kick { key: 7 }]
    );
}

#[test]
fn kicked_never_left_joins_again() {
    let mut gate = Gate::default();
    let start = Instant::now();
    let (stays, step) = gate.hello(7, SERVER, &said("you.trading-post@0.2.0"), start, rules());
    assert_eq!(
        (stays, step),
        (false, None),
        "no note on the loading screen"
    );
    assert_eq!(
        gate.joined(7, SERVER, at(start, 5), rules()),
        [note(
            7,
            true,
            vec![
                need("you.trading-post", "0.3.0", Some("0.2.0")),
                need("zones", "0.1.0", None)
            ]
        )]
    );
    assert_eq!(
        gate.due(SERVER, at(start, 15), rules()),
        [Step::Kick { key: 7 }]
    );
    // The player's slot never reported a leave; the same key enters again.
    assert_eq!(gate.joined(7, SERVER, at(start, 60), rules()), []);
    assert_eq!(
        gate.due(SERVER, at(start, 90), rules()),
        [note(7, false, all_lacking())],
        "the new join waits and is told again"
    );
    assert_eq!(
        gate.due(SERVER, at(start, 100), rules()),
        [Step::Kick { key: 7 }]
    );
}

#[test]
fn players_whose_slot_is_gone_are_forgotten() {
    let mut gate = Gate::default();
    let start = Instant::now();
    gate.joined(7, SERVER, start, rules());
    gate.hello(8, SERVER, &said("you.trading-post@0.2.0"), start, rules());
    gate.keep(&[9]);
    assert!(!gate.busy());
    assert_eq!(gate.due(SERVER, at(start, 60), rules()), []);
}

#[test]
fn grace_starts_when_the_player_enters_the_world() {
    let mut gate = Gate::default();
    let start = Instant::now();
    gate.hello(7, SERVER, &said(""), start, rules());
    assert_eq!(
        gate.due(SERVER, at(start, 100), rules()),
        [],
        "still loading"
    );
    assert_eq!(gate.joined(7, SERVER, at(start, 100), rules()).len(), 1);
    assert_eq!(gate.due(SERVER, at(start, 109), rules()), []);
    assert_eq!(
        gate.due(SERVER, at(start, 110), rules()),
        [Step::Kick { key: 7 }]
    );
}

#[test]
fn a_matching_hello_lets_the_player_stay_and_extra_mods_do_not_count() {
    let mut gate = Gate::default();
    let start = Instant::now();
    gate.joined(7, SERVER, start, rules());
    let client = said("camera@1.0.0,you.trading-post@0.3.0,zones@0.1.0");
    assert_eq!(gate.hello(7, SERVER, &client, start, rules()), (true, None));
    assert_eq!(gate.due(SERVER, at(start, 3600), rules()), []);
}

#[test]
fn a_matching_hello_before_the_join_is_answered_and_kept() {
    let mut gate = Gate::default();
    let start = Instant::now();
    assert_eq!(
        gate.hello(7, SERVER, &said(SERVER), start, rules()),
        (true, None)
    );
    assert_eq!(gate.joined(7, SERVER, start, rules()), []);
    assert_eq!(gate.due(SERVER, at(start, 3600), rules()), []);
}

#[test]
fn an_admitted_player_is_checked_again_when_the_server_s_mods_changed() {
    let mut gate = Gate::default();
    let start = Instant::now();
    gate.hello(
        7,
        "you.trading-post@0.3.0",
        &said("you.trading-post@0.3.0"),
        start,
        rules(),
    );
    assert_eq!(
        gate.joined(7, SERVER, start, rules()),
        [note(7, true, vec![need("zones", "0.1.0", None)])]
    );
}

#[test]
fn a_hello_that_lacks_a_mod_in_the_world_is_told_at_once_and_kicked() {
    let mut gate = Gate::default();
    let start = Instant::now();
    gate.joined(7, SERVER, start, rules());
    let (stays, step) = gate.hello(7, SERVER, &said("you.trading-post@0.2.0"), start, rules());
    assert!(!stays);
    assert_eq!(
        step,
        Some(note(
            7,
            true,
            vec![
                need("you.trading-post", "0.3.0", Some("0.2.0")),
                need("zones", "0.1.0", None)
            ]
        ))
    );
    assert_eq!(
        gate.hello(7, SERVER, &said("you.trading-post@0.2.0"), start, rules()),
        (false, None),
        "a repeated hello is not told again"
    );
    assert_eq!(
        gate.due(SERVER, at(start, 10), rules()),
        [Step::Kick { key: 7 }]
    );
}

#[test]
fn a_late_matching_hello_calls_the_kick_off() {
    let mut gate = Gate::default();
    let start = Instant::now();
    gate.joined(7, SERVER, start, rules());
    assert_eq!(gate.due(SERVER, at(start, 30), rules()).len(), 1);
    assert_eq!(
        gate.hello(7, SERVER, &said(SERVER), at(start, 31), rules()),
        (true, Some(Step::Stay { key: 7 }))
    );
    assert_eq!(gate.due(SERVER, at(start, 40), rules()), []);
}

#[test]
fn a_server_without_join_mods_admits_everyone() {
    let mut gate = Gate::default();
    let start = Instant::now();
    gate.joined(7, "", start, rules());
    assert_eq!(gate.due("", at(start, 60), rules()), []);
}

#[test]
fn an_unknown_sender_is_never_answered_or_told() {
    let mut gate = Gate::default();
    let start = Instant::now();
    assert_eq!(
        gate.hello(0, SERVER, &said(SERVER), start, rules()),
        (false, None)
    );
    assert_eq!(gate.joined(0, SERVER, start, rules()), []);
    assert_eq!(gate.due(SERVER, at(start, 60), rules()), []);
}

#[test]
fn a_full_gate_refuses_newcomers() {
    let mut gate = Gate::default();
    let start = Instant::now();
    for key in 1..=MAX_PLAYERS as u64 {
        gate.joined(key, SERVER, start, rules());
    }
    assert_eq!(gate.players.len(), MAX_PLAYERS);
    let key = MAX_PLAYERS as u64 + 1;
    assert_eq!(
        gate.joined(key, SERVER, start, rules()),
        [note(key, false, all_lacking()), Step::Kick { key }]
    );
    assert_eq!(
        gate.hello(key + 1, SERVER, &said(SERVER), start, rules()),
        (false, None)
    );
    assert_eq!(gate.players.len(), MAX_PLAYERS);
}

#[test]
fn a_full_gate_makes_room_from_players_whose_slot_is_gone() {
    let mut gate = Gate::default();
    let start = Instant::now();
    for key in 1..=MAX_PLAYERS as u64 {
        gate.joined(key, SERVER, start, rules());
    }
    let key = MAX_PLAYERS as u64 + 1;
    gate.make_room(1, || unreachable!("a followed player needs no room"));
    gate.make_room(key, || None);
    assert_eq!(
        gate.players.len(),
        MAX_PLAYERS,
        "no slots read, none forgotten"
    );
    gate.make_room(key, || Some(vec![2, key]));
    assert_eq!(gate.joined(key, SERVER, start, rules()), []);
    let mut followed: Vec<u64> = gate.players.keys().copied().collect();
    followed.sort_unstable();
    assert_eq!(followed, [2, key]);
    let mut roomy = Gate::default();
    roomy.joined(1, SERVER, start, rules());
    roomy.make_room(2, || unreachable!("a gate with room reads no slots"));
}

#[test]
fn a_hosted_game_acts_only_while_its_host_holds_a_live_slot() {
    let mut gate = Gate::hosted();
    let start = Instant::now();
    let host = Hello {
        nonce: 5,
        ..said("")
    };
    assert_eq!(gate.host_hello(1, &host), (true, None));
    gate.joined(2, SERVER, start, rules());
    assert_eq!(
        gate.due(SERVER, at(start, 60), rules()),
        [],
        "the host's key holds no slot"
    );
    gate.host_in_game(&[2, 3]);
    assert_eq!(gate.due(SERVER, at(start, 61), rules()), []);
    gate.host_in_game(&[1, 2]);
    assert_eq!(
        gate.due(SERVER, at(start, 62), rules()),
        [note(2, false, all_lacking())]
    );
    gate.host_in_game(&[2]);
    assert_eq!(
        gate.due(SERVER, at(start, 200), rules()),
        [],
        "the host left its slot"
    );
}

#[test]
fn the_host_s_machine_is_never_kicked() {
    let machines = [(1, 10), (2, 20), (3, 10)];
    assert_eq!(machine_to_kick(&machines, 2, Some(1), None), Ok(20));
    assert_eq!(machine_to_kick(&machines, 2, None, None), Ok(20));
    assert!(machine_to_kick(&machines, 1, Some(1), None).is_err());
    assert!(
        machine_to_kick(&machines, 3, Some(1), None).is_err(),
        "a guest on the host's machine"
    );
    assert!(machine_to_kick(&machines, 4, Some(1), None).is_err());
}

#[test]
fn a_player_without_an_account_is_kicked_only_when_no_other_slot_lacks_one() {
    assert_eq!(machine_to_kick(&[(0, 30), (2, 20)], 0, None, None), Ok(30));
    assert!(machine_to_kick(&[(0, 30), (2, 20), (0, 40)], 0, None, None).is_err());
}

#[test]
fn the_session_s_host_machine_is_never_kicked_before_the_host_said_hello() {
    let machines = [(1, 10), (2, 20)];
    assert!(machine_to_kick(&machines, 1, None, Some(10)).is_err());
    assert!(machine_to_kick(&machines, 2, Some(9), Some(20)).is_err());
    assert_eq!(machine_to_kick(&machines, 2, None, Some(10)), Ok(20));
}

#[test]
fn a_session_that_does_not_host_yet_refuses_no_kick() {
    let machines = [(2, 0)];
    assert_eq!(own_machine(0), None);
    assert_eq!(own_machine(7), Some(7));
    assert_eq!(machine_to_kick(&machines, 2, None, own_machine(0)), Ok(0));
}

#[test]
fn the_first_host_of_a_hosted_game_stays_its_host() {
    let mut gate = Gate::hosted();
    let start = Instant::now();
    let host = Hello {
        nonce: 5,
        ..said("")
    };
    assert_eq!(gate.host_hello(1, &host), (true, None));
    assert_eq!(gate.host_hello(2, &host), (false, None));
    assert_eq!(gate.host_hello(1, &host), (true, None));
    gate.host_in_game(&[1, 2]);
    gate.joined(2, SERVER, start, rules());
    assert_eq!(gate.joined(1, SERVER, start, rules()), []);
    assert_eq!(
        gate.due(SERVER, at(start, 30), rules()),
        [note(2, false, all_lacking())]
    );
}

#[test]
fn a_hosted_game_acts_once_it_knows_its_host_and_never_on_the_host() {
    let mut gate = Gate::hosted();
    let start = Instant::now();
    gate.joined(1, SERVER, start, rules());
    gate.joined(2, SERVER, start, rules());
    assert_eq!(gate.due(SERVER, at(start, 60), rules()), [], "no host yet");
    let host = Hello {
        nonce: 5,
        ..said("")
    };
    assert_eq!(gate.host_hello(1, &host), (true, None));
    gate.host_in_game(&[1, 2]);
    assert_eq!(
        gate.due(SERVER, at(start, 61), rules()),
        [note(2, false, all_lacking())]
    );
    assert_eq!(gate.joined(1, SERVER, at(start, 62), rules()), []);
    assert_eq!(
        gate.due(SERVER, at(start, 200), rules()),
        [Step::Kick { key: 2 }]
    );
}

#[test]
fn needs_name_the_server_s_mods_the_game_lacks_or_runs_otherwise() {
    assert_eq!(
        needs(SERVER, Some(&said("zones@0.1.0,other@1.0.0"))),
        [need("you.trading-post", "0.3.0", None)]
    );
    assert_eq!(needs(SERVER, Some(&said(SERVER))), []);
    assert_eq!(needs(SERVER, None).len(), 2);
    assert_eq!(needs("", None), []);
    assert_eq!(
        needs(SERVER, Some(&said("not a list"))).len(),
        2,
        "an unreadable hello lacks everything"
    );
}

#[test]
fn a_cut_hello_lacks_only_what_its_list_could_have_named() {
    let cut = Hello {
        mods: "area@1.0.0,you.trading-post@0.2.0".to_owned(),
        complete: false,
        nonce: 0,
        language: None,
    };
    let server = "area@1.0.0,camera@1.0.0,you.trading-post@0.3.0,zones@0.1.0";
    assert_eq!(
        needs(server, Some(&cut)),
        [
            need("camera", "1.0.0", None),
            need("you.trading-post", "0.3.0", Some("0.2.0"))
        ],
        "zones sorts after the cut"
    );
    let empty = Hello {
        mods: String::new(),
        complete: false,
        nonce: 0,
        language: None,
    };
    assert_eq!(needs(server, Some(&empty)).len(), 4);
}
