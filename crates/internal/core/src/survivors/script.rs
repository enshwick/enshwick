//! Survivors as a script reaches them: the lookup as an object for the call,
//! the workshop survivors as objects, and the hooks whose Rust arguments a
//! script cannot take as they are.

use core::ffi::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::{Answer, Lookup, NpcId, Purpose, WorkshopSurvivor};
use crate::content::Guid;
use crate::script::{Bound, Class, Function, Object};
use crate::{Entity, ItemId, Load, Off};

/// The script's answer to a lookup; nil or an error keeps the game's.
impl crate::script::Answer for Answer {
    const TY: &'static crate::script::Ty =
        &crate::script::Ty::Optional(<Self as crate::script::Typed>::TY);

    fn from_answer(answer: Result<crate::script::Value, crate::script::Error>) -> Self {
        answer
            .ok()
            .and_then(|value| crate::script::FromScript::from_value(value).ok())
            .unwrap_or(Self::Vanilla)
    }
}

/// The script's verdict on a summon; nil or an error lets it go on.
impl crate::script::Answer for super::SummonVerdict {
    const TY: &'static crate::script::Ty =
        &crate::script::Ty::Optional(<Self as crate::script::Typed>::TY);

    fn from_answer(answer: Result<crate::script::Value, crate::script::Error>) -> Self {
        answer
            .ok()
            .and_then(|value| crate::script::FromScript::from_value(value).ok())
            .unwrap_or(Self::Allow)
    }
}

/// A [`Lookup`] a script's handler gets: its fields, and `bed_of` while the
/// call lasts, since the loader's answer reaches only that far.
#[derive(Clone, Debug)]
pub(crate) struct ScriptLookup {
    survivor: NpcId,
    vanilla: Option<Entity>,
    purpose: Purpose,
    bed_of: Option<unsafe extern "system" fn(*mut c_void, Entity) -> Entity>,
    /// The loader's token, as an address: `bed_of` takes it back only while
    /// `live`.
    token: usize,
    live: Arc<AtomicBool>,
}

impl Object for ScriptLookup {
    const CLASS: &'static Class = &Class::of::<Self>(
        "Lookup",
        "A survivor lookup the game makes, for `on_survivor_lookup` to answer.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_LOOKUP];
}

#[crate::script(lookup)]
impl ScriptLookup {
    /// The survivor, as [`Lookup::survivor`] holds it.
    #[script(name = "survivor")]
    pub(crate) const fn survivor_script(&self) -> NpcId {
        self.survivor
    }

    /// The game's own answer, as [`Lookup::vanilla`] holds it.
    #[script(name = "vanilla")]
    pub(crate) const fn vanilla_script(&self) -> Option<Entity> {
        self.vanilla
    }

    /// Who asks, as [`Lookup::purpose`] holds it.
    #[script(name = "purpose")]
    pub(crate) const fn purpose_script(&self) -> Purpose {
        self.purpose
    }

    /// The bed `entity` sleeps in, as [`Lookup::bed_of`] tells; `None`
    /// once the handler's call is over.
    #[script(name = "bed_of")]
    pub(crate) fn bed_of_script(&self, entity: Entity) -> Option<Entity> {
        if !self.live.load(Ordering::Acquire) {
            return None;
        }
        Lookup {
            survivor: self.survivor,
            vanilla: self.vanilla,
            purpose: self.purpose,
            bed_of: self.bed_of,
            token: core::ptr::with_exposed_provenance_mut(self.token),
        }
        .bed_of(entity)
    }
}

impl Object for WorkshopSurvivor {
    const CLASS: &'static Class = &Class::of::<Self>(
        "WorkshopSurvivor",
        "A workshop survivor of the game, with its summoning staff and template.",
    );
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_WORKSHOP];
}

#[crate::script(workshop)]
impl WorkshopSurvivor {
    /// The short name, as [`WorkshopSurvivor::key`] gives it.
    #[script(name = "key")]
    pub(crate) fn key_script(&self) -> String {
        self.key().to_owned()
    }

    /// The survivor's id, as [`WorkshopSurvivor::id`] gives it.
    #[script(name = "id")]
    pub(crate) const fn id_script(&self) -> NpcId {
        self.id()
    }

    /// The vanilla summoning staff, as [`WorkshopSurvivor::staff`] gives it.
    #[script(name = "staff")]
    pub(crate) const fn staff_script(&self) -> Guid {
        self.staff()
    }

    /// The survivor's template, as [`WorkshopSurvivor::template`] gives it.
    #[script(name = "template")]
    pub(crate) const fn template_script(&self) -> Guid {
        self.template()
    }

    /// The survivor's name in `language`, as [`WorkshopSurvivor::name`]
    /// gives it.
    #[script(name = "name")]
    pub(crate) fn name_script(&self, language: crate::i18n::Language) -> Option<String> {
        self.name(language).map(str::to_owned)
    }
}

/// The free functions of [`survivors`](super) as a script's namespace
/// `survivors`.
#[doc(hidden)]
#[derive(Clone, Copy, Debug)]
pub struct SurvivorFns;

#[crate::script(survivor_fns)]
#[expect(
    clippy::unused_self,
    reason = "a script call takes its namespace, though a free function reads nothing of it"
)]
impl SurvivorFns {
    /// The survivor with the short name `key`, as
    /// [`WorkshopSurvivor::find`] finds it.
    #[script]
    #[must_use]
    pub fn find(self, key: &str) -> Option<WorkshopSurvivor> {
        WorkshopSurvivor::find(key).copied()
    }

    /// The survivor whose summoning staff has the game id `item`, as
    /// [`WorkshopSurvivor::by_staff`] finds it.
    #[script]
    #[must_use]
    pub fn by_staff(self, item: ItemId) -> Option<WorkshopSurvivor> {
        WorkshopSurvivor::by_staff(item).copied()
    }

    /// Every workshop survivor of the game, as
    /// [`WORKSHOP_SURVIVORS`](super::WORKSHOP_SURVIVORS) lists them.
    #[script]
    #[must_use]
    pub fn workshop_survivors(self) -> Vec<WorkshopSurvivor> {
        super::WORKSHOP_SURVIVORS.to_vec()
    }
}

impl Bound for SurvivorFns {
    const NAME: &'static str = "survivors";
    const DOC: &'static str = "Survivors: the game's lookups of them, their beds, summoning staffs and the workshop survivors.";
    const PARTS: &'static [&'static [Function<Self>]] = &[Self::SCRIPT_SURVIVOR_FNS];

    fn resolve(_load: &mut Load<'_>) -> Result<Self, Off> {
        Ok(Self)
    }
}

#[crate::script(survivor_hooks)]
impl Load<'_> {
    /// Makes `handler` the answer to the game's survivor lookups, as
    /// [`Load::on_survivor_lookup`] does: it gets the lookup as an object.
    ///
    /// # Errors
    ///
    /// As [`Load::on_survivor_lookup`].
    #[script(name = "on_survivor_lookup")]
    pub(crate) fn on_survivor_lookup_script(
        &mut self,
        handler: impl Fn(ScriptLookup) -> Answer + Send + Sync + 'static,
    ) -> Result<(), Off> {
        self.on_survivor_lookup(move |lookup| {
            let live = Arc::new(AtomicBool::new(true));
            let answer = handler(ScriptLookup {
                survivor: lookup.survivor,
                vanilla: lookup.vanilla,
                purpose: lookup.purpose,
                bed_of: lookup.bed_of,
                token: lookup.token.expose_provenance(),
                live: Arc::clone(&live),
            });
            live.store(false, Ordering::Release);
            answer
        })
    }

    /// Makes `merge` decide what the survivor menu shows, as
    /// [`Load::on_survivor_status`] does: it gets the status bytes as a
    /// list.
    ///
    /// # Errors
    ///
    /// As [`Load::on_survivor_status`].
    #[script(name = "on_survivor_status")]
    pub(crate) fn on_survivor_status_script(
        &mut self,
        merge: impl Fn(NpcId, Vec<u8>) -> Option<u8> + Send + Sync + 'static,
    ) -> Result<(), Off> {
        self.on_survivor_status(move |survivor, statuses| merge(survivor, statuses.to_vec()))
    }
}
