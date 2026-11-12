//! A mod names none of what the internal crates share only among
//! themselves, and cannot make a `Loading` of its own.

use enshwick::WorldError;
use enshwick::abi::{ModHandle, Status};
use enshwick::content::Guid;
use enshwick::content::vanilla::Vanilla;
use enshwick::content::workshops::NewWorkshop;
use enshwick::ecs::ComponentType;
use enshwick::game_events::CraftRecipeEvent;
use enshwick::schedule::Scheduler;
use enshwick::{Entity, Host, RecipeId};

fn reach_content(vanilla: &Vanilla, workshop: &NewWorkshop, guid: Guid) {
    let _ = vanilla.is_item(guid);
    let _ = vanilla.is_recipe(guid);
    let _ = workshop.check();
}

fn reach_world(status: Status) {
    let _ = WorldError::from_status(status);
    let _ = WorldError::check(status, WorldError::Missing);
}

fn reach_inventory(host: Host) {
    let _ = enshwick::inventory::item_data(&host, enshwick::ItemId::new(1));
    let _ = enshwick::loot::replace_group(0, Vec::new());
    let _: Option<enshwick::survivors::script::ScriptLookup> = None;
}

fn reach_inventory_scripts(load: &mut enshwick::Load<'_>) {
    use enshwick::Loading as _;
    use enshwick::damage::LoadMethods as _;
    use enshwick::entity_data::LoadMethods as _;
    use enshwick::item_data::LoadMethods as _;
    use enshwick::loot::LoadMethods as _;
    let base = load.base_mut();
    let _ = base.on_damage_script(|_| ());
    let _ = base.on_loot_pass_script(|_| ());
    let _ = base.item_data_script("kind");
    let _ = base.entity_data_script("kind", enshwick::entity_data::Replication::Server);
}

fn reach_knowledge(load: &mut enshwick::Load<'_>, reward: enshwick::rewards::Reward) {
    use enshwick::Loading as _;
    use enshwick::knowledge::LoadMethods as _;
    use enshwick::knowledge::script::LoadMethods as _;
    use enshwick::progress::LoadMethods as _;
    enshwick::rewards::replace(ModHandle(1), reward);
    let _: Option<enshwick::progress::script::ScriptLevels> = None;
    let base = load.base_mut();
    let _ = base.on_knowledge_input_script(|_| None);
    let _ = base.on_comfort_script(|_| None);
    let _ = base.on_experience_script(|_, _| None);
}

fn reach_actions(load: &mut enshwick::Load<'_>, texts: enshwick::i18n::Texts) {
    use enshwick::Loading as _;
    use enshwick::actions::script::LoadMethods as _;
    use enshwick::controls::script::LoadMethods as _;
    let _: Option<enshwick::controls::script::ScriptControl> = None;
    let _: Option<enshwick::actions::script::ScriptOutcome> = None;
    let base = load.base_mut();
    let _ = base.actions_script(texts);
    let _ = base.control_script((), |_| ());
}

fn reach(host: Host, scheduler: Scheduler, ty: ComponentType) {
    let _ = host.install();
    host.scope(|| ());
    let _ = Scheduler::get(ModHandle(1));
    scheduler.run_due(0);
    let _ = ty.read(host, Entity(2));
    let _ = CraftRecipeEvent::new(Entity(2), None, RecipeId::new(1), 1);
    enshwick::__private::as_loader(|| ());
    enshwick::__private::refresh_levels();
    let _ = enshwick::__private::install_host;
    let _ = enshwick::__private::as_pack::<()>;
}

struct Mine;

impl enshwick::Loading for Mine {}

fn main() {}
