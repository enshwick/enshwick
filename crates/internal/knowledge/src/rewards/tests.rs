use super::*;

#[test]
fn a_limit_holds_once_per_player_per_day_or_per_world() {
    let once = Reward::new("first", Action::Join)
        .with(Give::Experience(1))
        .with_limit(Limit::Once);
    let daily = Reward::new("daily", Action::Loot)
        .with(Give::Experience(1))
        .with_limit(Limit::Daily);
    let world = Reward::new("world", Action::AnyQuest)
        .with(Give::Experience(1))
        .with_limit(Limit::World);
    let mut given = Given::default();
    for reward in [&once, &daily, &world] {
        assert!(given.allows(reward, 7, 100));
        given.note(reward, 7, 100);
        assert!(!given.allows(reward, 7, 100), "{}", reward.name);
    }
    assert!(given.allows(&once, 8, 100), "another player still gets it");
    assert!(given.allows(&daily, 7, 101), "the next day gives it again");
    assert!(!given.allows(&world, 8, 100), "once in the world");
}

#[test]
fn pending_experience_comes_out_a_level_room_at_a_time_per_player() {
    let pending = Pending::new();
    assert!(pending.add(Entity::from_engine(5), 120));
    assert!(pending.add(Entity::from_engine(6), 10));
    assert!(pending.add(Entity::from_engine(5), 30));
    assert_eq!(pending.take(Entity::from_engine(5), 100), 100);
    assert_eq!(pending.take(Entity::from_engine(5), 0), 0);
    assert_eq!(pending.take(Entity::from_engine(5), 100), 50);
    assert_eq!(
        pending.take(Entity::from_engine(5), 100),
        0,
        "a slot frees once taken"
    );
    assert_eq!(pending.take(Entity::from_engine(6), 100), 10);
    for player in 1..=32 {
        assert!(pending.add(Entity::from_engine(100 + player), 1));
    }
    assert!(
        !pending.add(Entity::from_engine(500), 1),
        "every slot is held"
    );
}

#[test]
fn a_success_pays_the_oldest_open_request_up_to_its_amount() {
    let (one, two) = (Entity::from_engine(1), Entity::from_engine(2));
    // Player 1 asks for three in the delivery after step 4, player 2 for
    // one in the one after step 5.
    let mut asked = VecDeque::from([(one, 4, 3), (two, 5, 1)]);
    assert_eq!(crafted_by(&mut asked, 5), Some(one));
    assert_eq!(crafted_by(&mut asked, 5), Some(one));
    assert_eq!(crafted_by(&mut asked, 5), Some(one));
    assert_eq!(
        crafted_by(&mut asked, 6),
        Some(two),
        "player 1 had all three"
    );
    assert_eq!(
        crafted_by(&mut asked, 6),
        None,
        "a fifth is a station's own"
    );
    assert_eq!(crafted_by(&mut asked, 9), None);
    assert!(asked.is_empty(), "a request without successes goes");
}

#[test]
fn a_success_a_step_off_the_static_reading_still_pays_its_request() {
    let mut own_delivery = VecDeque::from([(Entity::from_engine(1), 7, 1)]);
    assert_eq!(
        crafted_by(&mut own_delivery, 7),
        Some(Entity::from_engine(1))
    );
    let mut a_step_late = VecDeque::from([(Entity::from_engine(1), 7, 1)]);
    assert_eq!(
        crafted_by(&mut a_step_late, 9),
        Some(Entity::from_engine(1))
    );
    let mut too_late = VecDeque::from([(Entity::from_engine(1), 7, 1)]);
    assert_eq!(crafted_by(&mut too_late, 10), None);
}

#[test]
fn a_refused_request_never_takes_another_players_success() {
    // Player 1's request was refused: no success came for it.
    let mut asked = VecDeque::from([
        (Entity::from_engine(1), 2, 1),
        (Entity::from_engine(2), 3, 1),
    ]);
    assert_eq!(crafted_by(&mut asked, 5), Some(Entity::from_engine(2)));
    assert!(
        asked
            .iter()
            .all(|&(player, _, _)| player != Entity::from_engine(1))
    );
}

#[test]
fn a_success_before_any_request_pays_nobody() {
    let mut asked = Asked::new();
    assert_eq!(crafted_by(&mut asked, 1), None);
    let mut later = VecDeque::from([(Entity::from_engine(1), 8, 1)]);
    assert_eq!(
        crafted_by(&mut later, 7),
        None,
        "a success before the request's delivery is not its"
    );
}

#[test]
fn a_player_keeps_one_slot_and_entity_zero_holds_none() {
    let pending = Pending::new();
    assert!(pending.add(Entity::from_engine(1), 5));
    assert!(pending.add(Entity::from_engine(2), 5));
    assert_eq!(pending.take(Entity::from_engine(1), 10), 5);
    assert!(
        pending.add(Entity::from_engine(2), 7),
        "the free first slot is not taken twice"
    );
    assert_eq!(
        pending.take(Entity::from_engine(2), 100),
        12,
        "one slot holds all of it"
    );
    assert!(!pending.add(Entity::from_engine(0), 3));
    assert!(pending.add(Entity::from_engine(9), 4));
    assert_eq!(
        pending.take(Entity::from_engine(0), 100),
        0,
        "no other player's experience"
    );
}

#[test]
fn a_player_who_leaves_or_a_new_world_frees_the_slots() {
    let pending = Pending::new();
    for player in 1..=32 {
        assert!(pending.add(Entity::from_engine(player), 1));
    }
    pending.forget(Entity::from_engine(7));
    pending.forget(Entity::from_engine(0));
    assert!(
        pending.add(Entity::from_engine(40), 1),
        "the leaver's slot is free"
    );
    assert!(!pending.add(Entity::from_engine(41), 1));
    assert_eq!(
        pending.take(Entity::from_engine(7), 10),
        0,
        "nothing kept for them"
    );
    pending.clear();
    assert_eq!(pending.take(Entity::from_engine(1), 10), 0);
    assert!(
        pending.add(Entity::from_engine(41), 1),
        "a new world starts empty"
    );
}

#[test]
fn a_reward_without_a_gift_a_bad_name_or_chance_is_refused() {
    assert!(Reward::new("none", Action::Join).problem().is_some());
    let ok = Reward::new("ok", Action::Join).with(Give::Experience(1));
    assert_eq!(ok.problem(), None);
    assert!(ok.clone().with_chance(1.5).problem().is_some());
    let mut bad = ok;
    bad.name = "Bad Name".to_owned();
    assert!(bad.problem().is_some());
}
