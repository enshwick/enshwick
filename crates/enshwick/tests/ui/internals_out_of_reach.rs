//! A mod names none of what the internal crates share only among
//! themselves, and cannot make a `Loading` of its own.

use enshwick::abi::ModHandle;
use enshwick::ecs::ComponentType;
use enshwick::game_events::CraftRecipeEvent;
use enshwick::schedule::Scheduler;
use enshwick::{Entity, Host, RecipeId};

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
