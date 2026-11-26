//! Knowledge rules in the game's own trigger list: when a query holds, the
//! game grants knowledge, with no mod code running then.
//!
//! The game's server runs its world rules with no player and its player
//! rules for each player, every 32 steps, and grants each rule's knowledge
//! while its query holds and the knowledge does not; a rule whose query id
//! is 0 is skipped (server 26430253, read in the code). So a removed rule
//! keeps its place with query 0, and the indices of the game's rules stay
//! for every mod after it.

use std::sync::{Arc, Mutex};

use crate::abi::{Level, ResourceKey};
use crate::content::knowledge::KnowledgeQuery;
use crate::content::{EngineFacts as _, Id};
use crate::knowledge::{Grant, KnowledgeId, Scope};
use crate::locks::Relock as _;
use crate::resources::{Registry, Value};
use crate::typed::{
    GameKnowledgeQueryTrigger, GameKnowledgeQueryTriggerResource, Named as _, Reflected as _,
};
use crate::{Host, Off};
use enshwick_content::__load::*;

const WORLD: &str = "worldQueries";
const PLAYER: &str = "playerQueries";
const QUERY: &str = "query";
const UNLOCK: &str = "unlockKnowledge";
const EVENT: &str = "requiresEvent";

/// A rule of the game's trigger list: its side and its place there.
///
/// # Examples
///
/// ```
/// use enshwick::knowledge::{RuleId, Scope};
///
/// let first = RuleId { scope: Scope::Player, index: 0 };
/// assert_eq!(first.index, 0);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[expect(
    clippy::exhaustive_structs,
    reason = "a rule's place: its side and its index there"
)]
pub struct RuleId {
    /// Its list: the world's rules, or each player's.
    pub scope: Scope,
    /// Its place in the list, from 0.
    pub index: u32,
}

/// One grant of a knowledge rule: an entry and what the rule grants of it.
///
/// # Examples
///
/// See [`NewRule`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
#[non_exhaustive]
pub struct RuleGrant {
    /// The entry granted; the game grants only an entry its knowledge
    /// holds, the game's or a mod's.
    pub id: KnowledgeId,
    /// What of it.
    pub grant: Grant,
}

impl RuleGrant {
    /// A `RuleGrant` of its parts.
    ///
    /// # Examples
    ///
    /// See [`RuleGrant`].
    #[must_use]
    pub const fn new(id: KnowledgeId, grant: Grant) -> Self {
        Self { id, grant }
    }
}

/// A new knowledge rule for [`Load::add_knowledge_rule`](../struct.Load.html#method.add_knowledge_rule).
///
/// The name is enshwick's; the game's is `GameKnowledgeQueryTrigger`.
///
/// # Examples
///
/// ```
/// use enshwick::content::id;
/// use enshwick::content::knowledge::KnowledgeQuery;
/// use enshwick::knowledge::{Grant, KnowledgeId, NewRule, RuleGrant, Scope};
///
/// fn init(load: &mut enshwick::Load<'_>, both: KnowledgeQuery, crowned: KnowledgeId) -> Result<(), enshwick::Off> {
///     load.add_knowledge_rule(NewRule::new(id!("crown-on-both"), Scope::Player, both, vec![RuleGrant::new(crowned, Grant::Set)]))
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct NewRule {
    /// Its content id, for the loader's refusals and logs.
    pub id: Id,
    /// Whose it is: a player's rule runs for each player, a world's for
    /// the world.
    pub scope: Scope,
    /// When it grants: a game query, such as one of
    /// [`Load::add_query`](../struct.Load.html#method.add_query), or a test
    /// of one entry.
    pub when: KnowledgeQuery,
    /// What it grants: bools set and flags; a counter is refused, since the
    /// game adds to it on every run while the query holds.
    pub grant: Vec<RuleGrant>,
}

impl NewRule {
    /// A `NewRule` of its parts.
    ///
    /// # Examples
    ///
    /// See [`NewRule`].
    #[must_use]
    pub const fn new(id: Id, scope: Scope, when: KnowledgeQuery, grant: Vec<RuleGrant>) -> Self {
        Self {
            id,
            scope,
            when,
            grant,
        }
    }
}

/// A change of a vanilla rule for [`Load::change_knowledge_rule`](../struct.Load.html#method.change_knowledge_rule); `None`
/// keeps what the rule holds.
///
/// # Examples
///
/// See [`Load::change_knowledge_rule`](../struct.Load.html#method.change_knowledge_rule).
#[derive(Clone, Debug, Default, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct RuleChange {
    /// When it grants.
    pub when: Option<KnowledgeQuery>,
    /// What it grants, in place of its own.
    pub grant: Option<Vec<RuleGrant>>,
}

/// A rule of the game's trigger list, as the game holds it.
///
/// # Examples
///
/// ```
/// fn bools(load: &mut enshwick::Load<'_>) -> Result<usize, enshwick::Off> {
///     Ok(load.knowledge_rules()?.iter().map(|rule| rule.grant.len()).sum())
/// }
/// ```
#[derive(Clone, Debug, PartialEq, Eq, crate::Script)]
#[non_exhaustive]
pub struct RuleInfo {
    /// Its side and place.
    pub id: RuleId,
    /// When it grants; `None` for a rule the game skips.
    pub when: Option<KnowledgeQuery>,
    /// What it grants; a grant of a kind enshwick does not read is left out.
    pub grant: Vec<RuleGrant>,
    /// Whether it waits for an event too, which this API does not write.
    pub on_event: bool,
}

/// The list of `scope`'s rules.
const fn list_name(scope: Scope) -> &'static str {
    if matches!(scope, Scope::Player) {
        PLAYER
    } else {
        WORLD
    }
}

/// The game's unlock operation of `grant` on `id`.
fn unlock(owner: &str, id: KnowledgeId, grant: Grant) -> Result<Value, Off> {
    let (kind, value) = match grant {
        Grant::Set => ("Bool", 1),
        Grant::Flag(bit) if bit < 32 => ("Flags", 1u32 << bit),
        Grant::Flag(bit) => return Err(Off::new(format!("{owner}: no flag {bit}"))),
        Grant::Add(_) => {
            return Err(Off::new(format!(
                "{owner}: a rule adds to a counter on every run while its query holds"
            )));
        }
    };
    Ok(Value::Struct(vec![
        (
            "knowledgeId".into(),
            Value::Struct(vec![(
                "id".into(),
                Value::Struct(vec![("value".into(), Value::Uint(id.get().into()))]),
            )]),
        ),
        ("type".into(), Value::Name(kind.into())),
        ("value".into(), Value::Uint(value.into())),
    ]))
}

/// The grant the game's unlock operation `op` makes.
fn grant_of(op: &Value) -> Option<RuleGrant> {
    let id = op.field("knowledgeId")?.field("id")?.field("value")?;
    let Value::Uint(id) = id else { return None };
    let Some(Value::Uint(value)) = op.field("value") else {
        return None;
    };
    let grant = match op.field("type") {
        Some(Value::Name(kind)) if kind == "Bool" => Grant::Set,
        Some(Value::Name(kind)) if kind == "Flags" && value.is_power_of_two() => {
            Grant::Flag(value.trailing_zeros())
        }
        Some(Value::Name(kind)) if kind == "Counter" => Grant::Add(u32::try_from(*value).ok()?),
        _ => return None,
    };
    Some(RuleGrant {
        id: KnowledgeId::new(u32::try_from(*id).ok()?),
        grant,
    })
}

/// The unlock operations of `grant`, each checked.
fn unlocks(owner: &str, grant: &[RuleGrant]) -> Result<Vec<Value>, Off> {
    if grant.is_empty() {
        return Err(Off::new(format!("{owner}: grants nothing")));
    }
    grant
        .iter()
        .map(|grant| unlock(owner, grant.id, grant.grant))
        .collect()
}

/// One change a mod makes to the trigger list.
#[derive(Debug)]
enum Edit {
    /// A rule of its own, its query and unlocks written.
    Add(Scope, Value, Vec<Value>),
    /// A change of a vanilla rule.
    Change(RuleId, Option<Value>, Option<Vec<Value>>),
    /// A removal of a vanilla rule.
    Remove(RuleId),
}

/// The rule `id` of `list`, to change.
fn rule_mut(triggers: &mut Value, id: RuleId) -> Result<&mut Value, Off> {
    let Some(Value::List(list)) = triggers.field_mut(list_name(id.scope)) else {
        return Err(Off::new("knowledge rules: no list of rules"));
    };
    usize::try_from(id.index)
        .ok()
        .and_then(|at| list.get_mut(at))
        .ok_or_else(|| {
            Off::new(format!(
                "knowledge rule {:?} {}: no such rule",
                id.scope, id.index
            ))
        })
}

/// Sets field `name` of `value`; a field the build lacks is an error.
fn set(value: &mut Value, name: &str, new: Value) -> Result<(), Off> {
    *value
        .field_mut(name)
        .ok_or_else(|| Off::new(format!("knowledge rules: no field {name}")))? = new;
    Ok(())
}

/// A query no knowledge holds, which the game skips.
fn skipped(template: &Value) -> Value {
    let mut query = template.clone();
    if let Some(id) = query
        .field_mut("knowledgeOrQueryId")
        .and_then(|id| id.field_mut("value"))
    {
        *id = Value::Uint(0);
    }
    query
}

impl Edit {
    /// Makes the change in `triggers`; on an error they are left as they
    /// were.
    fn apply(&self, triggers: &mut Value) -> Result<(), Off> {
        match self {
            Self::Add(scope, query, unlocks) => {
                let template = [WORLD, PLAYER]
                    .iter()
                    .find_map(|list| match triggers.field(list) {
                        Some(Value::List(rules)) => rules.first().cloned(),
                        _ => None,
                    })
                    .ok_or_else(|| Off::new("knowledge rules: no rule to copy"))?;
                let mut rule =
                    enshwick_content::__internal::blanked::<GameKnowledgeQueryTrigger>(&template);
                set(&mut rule, QUERY, query.clone())?;
                set(&mut rule, UNLOCK, Value::List(unlocks.clone()))?;
                set(&mut rule, EVENT, Value::Bool(false))?;
                let Some(Value::List(list)) = triggers.field_mut(list_name(*scope)) else {
                    return Err(Off::new("knowledge rules: no list of rules"));
                };
                list.push(rule);
                Ok(())
            }
            Self::Change(id, query, unlocks) => {
                let mut rule = rule_mut(triggers, *id)?.clone();
                if let Some(query) = query {
                    set(&mut rule, QUERY, query.clone())?;
                }
                if let Some(unlocks) = unlocks {
                    set(&mut rule, UNLOCK, Value::List(unlocks.clone()))?;
                }
                *rule_mut(triggers, *id)? = rule;
                Ok(())
            }
            Self::Remove(id) => {
                let rule = rule_mut(triggers, *id)?;
                let query = rule
                    .field(QUERY)
                    .map(skipped)
                    .ok_or_else(|| Off::new("knowledge rules: a rule without a query"))?;
                set(rule, QUERY, query)
            }
        }
    }
}

/// The rules of the trigger list `triggers`, the world's and the players'.
fn rules_of(triggers: &Value) -> Vec<RuleInfo> {
    [Scope::World, Scope::Player]
        .into_iter()
        .flat_map(|scope| {
            let rules = match triggers.field(list_name(scope)) {
                Some(Value::List(rules)) => rules.as_slice(),
                _ => &[],
            };
            rules.iter().zip(0..).map(move |(rule, index)| {
                let when = rule
                    .field(QUERY)
                    .and_then(|query| crate::typed::KnowledgeQueryData::from_value(query).ok())
                    .and_then(|data| enshwick_content::__internal::query_from_data(&data));
                let grant = match rule.field(UNLOCK) {
                    Some(Value::List(ops)) => ops.iter().filter_map(grant_of).collect(),
                    _ => Vec::new(),
                };
                RuleInfo {
                    id: RuleId { scope, index },
                    when,
                    grant,
                    on_event: matches!(rule.field(EVENT), Some(Value::Bool(true))),
                }
            })
        })
        .collect()
}

/// The trigger list's patch of a mod, served with its first change.
#[derive(Debug)]
pub(crate) struct Rules(Arc<Mutex<Vec<Edit>>>);

/// The key of the game's trigger list, the one resource of its type, a
/// server's resource only.
fn trigger_list(host: Host) -> Result<ResourceKey, Off> {
    let hash = host.hash(GameKnowledgeQueryTriggerResource::ID)?;
    let ty = Registry::game()?
        .hashed(hash)
        .ok_or_else(|| Off::new("knowledge rules: the list's type is not reflected"))?;
    match host.resource_keys(Some(ty))?.as_slice() {
        [one] => Ok(*one),
        keys => Err(Off::new(format!(
            "knowledge rules: {} lists in the archive, not one",
            keys.len()
        ))),
    }
}

/// `Load`'s rule methods on the base handle, which core's forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::add_knowledge_rule`.
    #[doc(hidden)]
    fn add_knowledge_rule(&mut self, new: NewRule) -> Result<(), Off>;

    /// As `Load::knowledge_rules`.
    #[doc(hidden)]
    fn knowledge_rules(&mut self) -> Result<Vec<RuleInfo>, Off>;

    /// As `Load::change_knowledge_rule`.
    #[doc(hidden)]
    fn change_knowledge_rule(&mut self, rule: RuleId, change: RuleChange) -> Result<(), Off>;

    /// As `Load::remove_knowledge_rule`.
    #[doc(hidden)]
    fn remove_knowledge_rule(&mut self, rule: RuleId) -> Result<(), Off>;
}

/// Adds `edit` to the mod's patch of the trigger list once it applies to
/// the game's list.
fn edit(load: &mut crate::BaseLoad, edit: Edit) -> Result<(), Off> {
    let host = load.host();
    edit.apply(&mut host.game_value(trigger_list(host)?)?)?;
    push(load, edit)
}

/// Adds the checked `edit` to the mod's patch of the trigger list, serving
/// the patch with the first.
fn push(load: &mut crate::BaseLoad, edit: Edit) -> Result<(), Off> {
    let host = load.host();
    let key = trigger_list(host)?;
    let rules = match load.slot::<Rules>() {
        Some(rules) => rules,
        empty => empty.insert(Rules(enshwick_content::__internal::serve_list(
            host,
            key,
            move |mut triggers, edits: &[Edit]| {
                for edit in edits {
                    if let Err(off) = edit.apply(&mut triggers) {
                        host.log(Level::WARN, &off.to_string());
                    }
                }
                Ok(triggers)
            },
        )?)),
    };
    rules.0.relock().push(edit);
    Ok(())
}

impl LoadMethods for crate::BaseLoad {
    fn add_knowledge_rule(&mut self, new: NewRule) -> Result<(), Off> {
        let owner = format!("knowledge rule {}", new.id);
        let query = enshwick_content::__internal::query_value(new.when)?;
        let unlocks = unlocks(&owner, &new.grant)?;
        let checked = Edit::Add(new.scope, query, unlocks);
        let host = self.host();
        // Checked before the declare, so a refused rule takes no id.
        checked.apply(&mut host.game_value(trigger_list(host)?)?)?;
        self.declare(new.id)?;
        push(self, checked)
    }

    fn knowledge_rules(&mut self) -> Result<Vec<RuleInfo>, Off> {
        let host = self.host();
        Ok(rules_of(&host.game_value(trigger_list(host)?)?))
    }

    fn change_knowledge_rule(&mut self, rule: RuleId, change: RuleChange) -> Result<(), Off> {
        let owner = format!("knowledge rule {:?} {}", rule.scope, rule.index);
        let query = change
            .when
            .map(enshwick_content::__internal::query_value)
            .transpose()?;
        let unlocks = change
            .grant
            .as_deref()
            .map(|grant| unlocks(&owner, grant))
            .transpose()?;
        edit(self, Edit::Change(rule, query, unlocks))
    }

    fn remove_knowledge_rule(&mut self, rule: RuleId) -> Result<(), Off> {
        edit(self, Edit::Remove(rule))
    }
}

#[cfg(test)]
mod tests;
