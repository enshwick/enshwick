//! The crafting and item commands through the commands' frame.

use super::*;
use crate::actions::client_control::Act;

#[test]
fn a_mod_runs_one_craft_or_item_request_at_a_time_on_any_screen() {
    let craft = Kind::Act(Act::Craft {
        station: 0,
        recipe: 0x7cd8_0a04,
        count: 1,
    });
    assert!(!craft.waits_beside(Kind::Act(Act::Trash { slot: 0x43 })));
    assert!(craft.waits_beside(Kind::Mark(Mark::Clear)));
    assert!(craft.handed() && !craft.drives());
    let command = Command {
        expected: None,
        ..command(craft)
    };
    let menu = seen(4, TabRead::NoMenu);
    assert_eq!(next(&command, &menu, None, 150), Next::Apply);
    let handed = Command {
        handed: Some(150),
        ..command
    };
    assert_eq!(
        next(&handed, &menu, Some(Progress::Taken), 160),
        Next::Applied,
        "applied once written"
    );
    assert_eq!(
        next(&handed, &menu, Some(Progress::Done), 170),
        Next::End(Status::OBSERVED, 0),
        "observed once the inventory changed"
    );
}
