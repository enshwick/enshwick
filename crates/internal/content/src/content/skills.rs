//! Skill nodes and perks from mods.
//!
//! [`Load::add_skill_node`](../../struct.Load.html#method.add_skill_node) adds a node to the skill tree: a copy of a
//! vanilla node's effect at a place of its own, linked from vanilla or mod
//! nodes, with its own texts, cost and levels. A player's save keeps room
//! for a fixed count of unlocked nodes, read from the game's reflection, so
//! the few free places are shared by every mod and a node beyond them is
//! refused. [`Load::add_perk`](../../struct.Load.html#method.add_perk) adds a copy of a vanilla perk with its own
//! texts. [`Load::skill_nodes`](../../struct.Load.html#method.skill_nodes) lists the vanilla nodes to link from.
//!
//! ```
//! use enshwick::content::id;
//! use enshwick::content::skills::NewSkillNode;
//! use enshwick::typed::SkillNodeType;
//!
//! fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
//!     let tree = load.skill_nodes()?;
//!     let (Some(root), Some(stamina)) = (
//!         tree.iter().find(|node| node.kind == SkillNodeType::Root),
//!         tree.iter().find(|node| node.kind == SkillNodeType::Attribute),
//!     ) else {
//!         return Ok(());
//!     };
//!     load.add_skill_node(NewSkillNode {
//!         links_from: vec![root.id],
//!         ..NewSkillNode::new(id!("endurance"), stamina.id, root.x + 120.0, root.y, id!("endurance.name").guid(), id!("endurance.text").guid())
//!     })?;
//!     Ok(())
//! }
//! ```

use crate::__load::*;
use crate::Off;
use crate::content::knowledge::{hash_key, key_of as id_of};
use crate::resources::{Registry, Value};
use crate::typed::ecs::UnlockedSkillNodes;
use crate::typed::{
    Named as _, PerkCollectionResource, Reflected as _, SkillNodeType, SkillTreeResource,
};

use super::{EngineFacts, Guid, Id};

/// The game's skill tree (server 387 and 399, `SkillTreeResource`).
pub const SKILL_TREE: Guid = Guid::parse("108ac23f-a77d-4f18-bfac-acc397edaaf0");
/// The game's list of every perk (server 387 and 399, `PerkCollectionResource`).
pub const PERK_COLLECTION: Guid = Guid::parse("bcaac92e-70bf-4fdb-8119-001678e7a616");

/// The game's reflected name of a perk, which the generated types hold as a
/// struct but not as a resource.
const PERK: &str = "Perk";

/// The game's names of what the tree, its nodes and its links hold.
const NODES: &str = "nodes";
const LINKS: &str = "links";
const ID: &str = "id";
const POSITION: &str = "uiPosition";
const INCOMING: &str = "incomingLinks";
const OUTGOING: &str = "outgoingLinks";
const KNOWN: &str = "knownSkillNodes";
const MAX_LEVEL: &str = "maxLevel";
const LEVEL_INDEX: &str = "levelIndex";

/// A node of the skill tree, vanilla or a mod's.
///
/// # Examples
///
/// ```
/// use enshwick::content::skills::SkillNodeId;
///
/// assert_eq!(SkillNodeId::new(7).get(), 7);
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, crate::Script)]
pub struct SkillNodeId(u32);

impl SkillNodeId {
    /// The node with id `id`.
    ///
    /// # Examples
    ///
    /// See [`SkillNodeId`].
    #[must_use]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Its id.
    ///
    /// # Examples
    ///
    /// See [`SkillNodeId`].
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// A vanilla node of the skill tree, from [`Load::skill_nodes`](../../struct.Load.html#method.skill_nodes).
///
/// The name is enshwick's: it holds a few fields of the game's
/// `SkillTreeNode`.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, crate::Script)]
#[non_exhaustive]
pub struct SkillNode {
    /// Its id.
    pub id: SkillNodeId,
    /// What it is: a root, an attribute, an active or a passive skill.
    pub kind: SkillNodeType,
    /// Where the tree screen draws it, across.
    pub x: f32,
    /// Where the tree screen draws it, down.
    pub y: f32,
}

/// A new skill node for [`Load::add_skill_node`](../../struct.Load.html#method.add_skill_node): a copy of the vanilla
/// node `like`, its effect and icons among it, with the values it gives.
///
/// # Examples
///
/// See the [module](self).
#[derive(Clone, Debug, PartialEq, crate::Script)]
pub struct NewSkillNode {
    /// Its content id; its node id is the id's GUID hashed as the game
    /// hashes ids.
    pub id: Id,
    /// The vanilla node copied.
    pub like: SkillNodeId,
    /// Where the tree screen draws it, across, in the tree's units: vanilla
    /// nodes lie about 100 to 6500 apart.
    pub x: f32,
    /// Where the tree screen draws it, down.
    pub y: f32,
    /// The nodes it is unlocked from, vanilla or a mod's.
    #[script(default)]
    pub links_from: Vec<SkillNodeId>,
    /// Its title's text tag.
    pub name: Guid,
    /// Its description's text tag.
    pub description: Guid,
    /// The skill points it costs; `None` keeps the vanilla node's.
    pub cost: Option<u16>,
    /// The levels it has; `None` keeps the vanilla node's. A node with
    /// levels is refused, since the game keeps them in a slot of the vanilla
    /// node's own, so a copy of one sets 0.
    pub max_level: Option<u8>,
    /// What it is; `None` keeps the vanilla node's.
    pub kind: Option<SkillNodeType>,
}

impl NewSkillNode {
    /// A copy of `like` under `id` at (`x`, `y`) titled `name`, linked from
    /// no node, keeping the vanilla node's cost, levels and kind.
    ///
    /// # Examples
    ///
    /// See the [module](self).
    #[must_use]
    pub const fn new(
        id: Id,
        like: SkillNodeId,
        x: f32,
        y: f32,
        name: Guid,
        description: Guid,
    ) -> Self {
        Self {
            id,
            like,
            x,
            y,
            links_from: Vec::new(),
            name,
            description,
            cost: None,
            max_level: None,
            kind: None,
        }
    }

    /// Its node id.
    fn node(&self) -> SkillNodeId {
        SkillNodeId(self.id.guid().fnv1a())
    }

    /// The id of its link from its `index`th node.
    fn link(&self, index: usize) -> u32 {
        Guid::named(&format!("{}.link-{index}", self.id)).fnv1a()
    }
}

/// A new perk for [`Load::add_perk`](../../struct.Load.html#method.add_perk): a copy of the vanilla perk `like`
/// with its own texts.
///
/// # Examples
///
/// See [`Load::add_perk`](../../struct.Load.html#method.add_perk).
#[derive(Clone, Copy, Debug, PartialEq, Eq, crate::Script)]
pub struct NewPerk {
    /// Its content id, its GUID the perk's.
    pub id: Id,
    /// The vanilla perk copied, by GUID.
    pub like: Guid,
    /// Its name's text tag.
    pub name: Guid,
    /// Its description's text tag.
    pub description: Guid,
}

/// The list in field `name` of `value`, to change.
fn list_mut<'a>(value: &'a mut Value, name: &str) -> Result<&'a mut Vec<Value>, Off> {
    match value.field_mut(name) {
        Some(Value::List(list)) => Ok(list),
        _ => Err(Off::new(format!("skill tree: no list of {name}"))),
    }
}

/// The list in field `name` of `value`.
fn list<'a>(value: &'a Value, name: &str) -> &'a [Value] {
    match value.field(name) {
        Some(Value::List(list)) => list,
        _ => &[],
    }
}

/// A node's place on the tree screen.
fn position(node: &Value) -> Option<(f64, f64)> {
    let at = node.field(POSITION)?;
    match (at.field("x"), at.field("y")) {
        (Some(Value::Float(x)), Some(Value::Float(y))) => Some((*x, *y)),
        _ => None,
    }
}

/// The nodes of `tree`, as [`Load::skill_nodes`](../../struct.Load.html#method.skill_nodes) lists them.
fn nodes_of(tree: &Value) -> Vec<SkillNode> {
    list(tree, NODES)
        .iter()
        .filter_map(|node| {
            let (x, y) = position(node)?;
            Some(SkillNode {
                id: SkillNodeId(id_of(node)?),
                kind: SkillNodeType::from_value(node.field("type")?).ok()?,
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "the codec holds the game's float32 exactly"
                )]
                x: x as f32,
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "the codec holds the game's float32 exactly"
                )]
                y: y as f32,
            })
        })
        .collect()
}

/// The closest a node may lie to another: half the smallest gap between
/// two of the tree's nodes, so the screen keeps them apart as the game's
/// own are (103 units on server 387).
fn spacing(nodes: &[(f64, f64)]) -> f64 {
    let mut closest = f64::INFINITY;
    for (i, a) in nodes.iter().enumerate() {
        for b in &nodes[i + 1..] {
            closest = closest.min((a.0 - b.0).hypot(a.1 - b.1));
        }
    }
    closest / 2.0
}

/// The node of `nodes` that `new` copies, or why `new` does not fit beside
/// them.
fn model<'a>(
    nodes: &'a [Value],
    new: &NewSkillNode,
    room: usize,
    near: f64,
) -> Result<&'a Value, Off> {
    let refuse = |why: String| Err(Off::new(format!("skill node {}: {why}", new.id)));
    if nodes.len() >= room {
        return refuse(format!(
            "the tree holds {} nodes, the most a player's save keeps",
            nodes.len()
        ));
    }
    let node = new.node();
    if nodes.iter().any(|held| id_of(held) == Some(node.get())) {
        return refuse(format!("the tree holds node {}", node.get()));
    }
    let Some(like) = nodes
        .iter()
        .find(|held| id_of(held) == Some(new.like.get()))
    else {
        return refuse(format!("no node {} to copy", new.like.get()));
    };
    let levels = match (new.max_level, like.field(MAX_LEVEL)) {
        (Some(levels), _) => u64::from(levels),
        (None, Some(Value::Uint(levels))) => *levels,
        (None, _) => 0,
    };
    if levels > 0 {
        return refuse(
            "a node with levels keeps them in a slot of the game's own (levelIndex), which a copy would share".into(),
        );
    }
    let (x, y) = (f64::from(new.x), f64::from(new.y));
    if !x.is_finite() || !y.is_finite() {
        return refuse(format!("({x}, {y}) is no place on the tree"));
    }
    if let Some((ox, oy)) = nodes
        .iter()
        .filter_map(position)
        .find(|&(ox, oy)| (ox - x).hypot(oy - y) < near)
    {
        return refuse(format!(
            "({x}, {y}) lies within {near} of the node at ({ox}, {oy})"
        ));
    }
    for (i, from) in new.links_from.iter().enumerate() {
        if !nodes.iter().any(|held| id_of(held) == Some(from.get())) {
            return refuse(format!("no node {} to link from", from.get()));
        }
        if new.links_from[..i].contains(from) {
            return refuse(format!("linked from node {} twice", from.get()));
        }
    }
    Ok(like)
}

/// `tree` with `new` added, unless the tree already holds `room` nodes or
/// `new` does not fit beside them (see [`model`]).
fn add_node_to(mut tree: Value, new: &NewSkillNode, room: usize, near: f64) -> Result<Value, Off> {
    let mut copy = model(list(&tree, NODES), new, room, near)?.clone();
    let node = new.node();
    let (x, y) = (f64::from(new.x), f64::from(new.y));
    let links: Vec<u32> = (0..new.links_from.len()).map(|i| new.link(i)).collect();
    let set = |copy: &mut Value, field: &str, value: Value| {
        if let Some(held) = copy.field_mut(field) {
            *held = value;
        }
    };
    set(&mut copy, ID, hash_key(node.get()));
    set(
        &mut copy,
        POSITION,
        Value::Struct(vec![
            ("x".into(), Value::Float(x)),
            ("y".into(), Value::Float(y)),
        ]),
    );
    set(&mut copy, "name", hash_key(new.name.fnv1a()));
    set(&mut copy, "description", hash_key(new.description.fnv1a()));
    set(
        &mut copy,
        INCOMING,
        Value::List(links.iter().map(|&l| hash_key(l)).collect()),
    );
    set(&mut copy, OUTGOING, Value::List(Vec::new()));
    // The slot every node without levels holds (server 387).
    set(&mut copy, LEVEL_INDEX, Value::Uint(0));
    if let Some(cost) = new.cost {
        set(&mut copy, "costs", Value::Uint(cost.into()));
    }
    if let Some(levels) = new.max_level {
        set(&mut copy, MAX_LEVEL, Value::Uint(levels.into()));
    }
    if let Some(kind) = &new.kind {
        set(&mut copy, "type", kind.to_value());
    }
    let all = list_mut(&mut tree, NODES)?;
    for (from, &link) in new.links_from.iter().zip(&links) {
        if let Some(source) = all.iter_mut().find(|held| id_of(held) == Some(from.get())) {
            list_mut(source, OUTGOING)?.push(hash_key(link));
        }
    }
    all.push(copy);
    let edges = list_mut(&mut tree, LINKS)?;
    for (from, link) in new.links_from.iter().zip(links) {
        edges.push(Value::Struct(vec![
            (ID.into(), hash_key(link)),
            ("sourceNode".into(), hash_key(from.get())),
            ("targetNode".into(), hash_key(node.get())),
            ("isBidirectional".into(), Value::Bool(false)),
        ]));
    }
    Ok(tree)
}

/// `Load`'s methods of this module on the base handle, which core's
/// forwarders call.
#[doc(hidden)]
pub trait LoadMethods {
    /// As `Load::skill_nodes`.
    #[doc(hidden)]
    fn skill_nodes(&self) -> Result<Vec<SkillNode>, Off>;

    /// As `Load::add_skill_node`.
    #[doc(hidden)]
    fn add_skill_node(&mut self, new: NewSkillNode) -> Result<SkillNodeId, Off>;

    /// As `Load::add_perk`.
    #[doc(hidden)]
    fn add_perk(&mut self, new: NewPerk) -> Result<Guid, Off>;
}

impl LoadMethods for crate::BaseLoad {
    fn skill_nodes(&self) -> Result<Vec<SkillNode>, Off> {
        Ok(nodes_of(&self.host().game_value(self.skill_tree()?)?))
    }

    fn add_skill_node(&mut self, new: NewSkillNode) -> Result<SkillNodeId, Off> {
        let host = self.host();
        let feature = format!("skill node {}", new.id);
        let checked = self.check_skill_node(&new);
        let (room, near, key) = report(host, &feature, checked)?;
        let node = new.node();
        crate::resources::serve_value(host, key, key, move |tree| {
            report(host, &feature, add_node_to(tree, &new, room, near))
        })?;
        Ok(node)
    }

    fn add_perk(&mut self, new: NewPerk) -> Result<Guid, Off> {
        let perk = self.hash(PERK)?;
        let collection = PERK_COLLECTION.key_by_hash(self.hash(PerkCollectionResource::ID)?);
        let like = new.like.key_by_hash(perk);
        if !list(&self.host().game_value(collection)?, "perks").contains(&Value::Guid(new.like)) {
            return Err(Off::new(format!(
                "perk {}: {} is no vanilla perk",
                new.id, new.like
            )));
        }
        let guid = self.declare(new.id)?;
        crate::resources::serve_value(
            self.host(),
            guid.key_by_hash(perk),
            like,
            move |mut value| {
                for (field, tag) in [("name", new.name), ("description", new.description)] {
                    if let Some(held) = value.field_mut(field) {
                        *held = Value::Guid(tag);
                    }
                }
                Ok(value)
            },
        )?;
        crate::resources::serve_value(self.host(), collection, collection, move |mut value| {
            list_mut(&mut value, "perks")?.push(Value::Guid(guid));
            Ok(value)
        })?;
        Ok(guid)
    }
}

/// The base handle's helpers of this module for the crate's own code.
pub(crate) trait LoadHelpers {
    /// The skill tree's resource key on the running build.
    fn skill_tree(&self) -> Result<crate::abi::ResourceKey, Off>;

    /// How many nodes a player's save keeps unlocked: the count of the
    /// saved unlocks' array, from the game's reflection.
    fn skill_room(&self) -> Result<usize, Off>;

    /// Checks `new` against the vanilla tree and declares its id: the room
    /// a save keeps, the spacing of the vanilla nodes and the tree's key.
    fn check_skill_node(
        &mut self,
        new: &NewSkillNode,
    ) -> Result<(usize, f64, crate::abi::ResourceKey), Off>;
}

impl LoadHelpers for crate::BaseLoad {
    fn skill_tree(&self) -> Result<crate::abi::ResourceKey, Off> {
        Ok(SKILL_TREE.key_by_hash(self.hash(SkillTreeResource::ID)?))
    }

    fn skill_room(&self) -> Result<usize, Off> {
        let registry = Registry::game()?;
        let unlocks = registry
            .hashed(self.hash(UnlockedSkillNodes::ID)?)
            .ok_or_else(|| Off::new("skill nodes: no saved unlocks on this build"))?;
        let field = unlocks
            .fields()
            .iter()
            .find(|field| field.name() == KNOWN)
            .ok_or_else(|| Off::new(format!("skill nodes: the saved unlocks hold no {KNOWN}")))?;
        field
            .type_name()
            .and_then(|name| registry.named(name))
            .and_then(crate::resources::Type::element_count)
            .and_then(|count| usize::try_from(count).ok())
            .ok_or_else(|| Off::new(format!("skill nodes: {KNOWN} is no array of a count")))
    }

    fn check_skill_node(
        &mut self,
        new: &NewSkillNode,
    ) -> Result<(usize, f64, crate::abi::ResourceKey), Off> {
        let room = self.skill_room()?;
        let key = self.skill_tree()?;
        let vanilla = self.host().game_value(key)?;
        let near = spacing(
            &list(&vanilla, NODES)
                .iter()
                .filter_map(position)
                .collect::<Vec<_>>(),
        );
        // A link from a node a mod adds is only in the tree once it is built.
        let known: Vec<_> = nodes_of(&vanilla).into_iter().map(|node| node.id).collect();
        let checked = NewSkillNode {
            links_from: new
                .links_from
                .iter()
                .copied()
                .filter(|from| known.contains(from))
                .collect(),
            ..new.clone()
        };
        add_node_to(vanilla, &checked, room, near)?;
        self.declare(new.id)?;
        Ok((room, near, key))
    }
}

/// Shows `outcome` as the mod's feature `feature` in the loader's status
/// summary and on its Mods screen: on, or off with the reason.
fn report<T>(host: crate::host::Host, feature: &str, outcome: Result<T, Off>) -> Result<T, Off> {
    let reason = outcome.as_ref().err().map_or("", Off::to_reason);
    let _shown = (host.api().feature)(
        host.handle(),
        crate::abi::Str::new(feature),
        crate::abi::Str::new(reason),
    );
    outcome
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Half the gap of the two nodes of [`tree`].
    const NEAR: f64 = 50.0;

    fn node(id: u32, x: f64, kind: &str) -> Value {
        Value::Struct(vec![
            (ID.into(), hash_key(id)),
            (
                POSITION.into(),
                Value::Struct(vec![
                    ("x".into(), Value::Float(x)),
                    ("y".into(), Value::Float(0.0)),
                ]),
            ),
            ("name".into(), hash_key(0)),
            ("description".into(), hash_key(0)),
            (INCOMING.into(), Value::List(Vec::new())),
            (OUTGOING.into(), Value::List(Vec::new())),
            ("costs".into(), Value::Uint(1)),
            ("type".into(), Value::Name(kind.into())),
            ("maxLevel".into(), Value::Uint(0)),
            ("levelIndex".into(), Value::Uint(u64::from(id))),
        ])
    }

    fn tree() -> Value {
        Value::Struct(vec![
            (
                NODES.into(),
                Value::List(vec![node(1, 0.0, "Root"), node(2, 100.0, "Attribute")]),
            ),
            (LINKS.into(), Value::List(Vec::new())),
        ])
    }

    fn new(name: &'static str, x: f32) -> NewSkillNode {
        NewSkillNode {
            links_from: vec![SkillNodeId(1)],
            cost: Some(2),
            ..NewSkillNode::new(
                Id::new(name),
                SkillNodeId(2),
                x,
                0.0,
                Guid([1; 16]),
                Guid([2; 16]),
            )
        }
    }

    #[test]
    fn a_node_copies_its_model_and_is_linked_both_ways_from_its_source() {
        let added = add_node_to(tree(), &new("you.skills:a", 300.0), 256, NEAR).unwrap();
        let nodes = list(&added, NODES);
        assert_eq!(nodes.len(), 3);
        let copy = &nodes[2];
        assert_eq!(id_of(copy), Some(Id::new("you.skills:a").guid().fnv1a()));
        assert_eq!(copy.field("type"), Some(&Value::Name("Attribute".into())));
        assert_eq!(copy.field("costs"), Some(&Value::Uint(2)));
        let link = &list(&added, LINKS)[0];
        assert_eq!(link.field("sourceNode"), Some(&hash_key(1)));
        assert_eq!(link.field("targetNode"), copy.field(ID));
        assert_eq!(list(&nodes[0], OUTGOING), [link.field(ID).unwrap().clone()]);
        assert_eq!(list(copy, INCOMING), [link.field(ID).unwrap().clone()]);
        assert!(
            list(&nodes[1], OUTGOING).is_empty(),
            "the model keeps its links"
        );
    }

    #[test]
    fn a_node_beyond_the_room_on_another_or_from_nowhere_is_refused() {
        assert!(
            add_node_to(tree(), &new("you.skills:a", 300.0), 2, NEAR).is_err(),
            "full"
        );
        assert!(
            add_node_to(tree(), &new("you.skills:a", 140.0), 256, NEAR).is_err(),
            "within 50 of 100"
        );
        assert!(add_node_to(tree(), &new("you.skills:a", 151.0), 256, NEAR).is_ok());
        let nowhere = NewSkillNode {
            links_from: vec![SkillNodeId(9)],
            ..new("you.skills:a", 300.0)
        };
        assert!(add_node_to(tree(), &nowhere, 256, NEAR).is_err());
        let unlike = NewSkillNode {
            like: SkillNodeId(9),
            ..new("you.skills:a", 300.0)
        };
        assert!(add_node_to(tree(), &unlike, 256, NEAR).is_err());
        let once = add_node_to(tree(), &new("you.skills:a", 300.0), 256, NEAR).unwrap();
        assert!(
            add_node_to(once, &new("you.skills:a", 600.0), 256, NEAR).is_err(),
            "twice"
        );
    }

    #[test]
    fn a_node_off_the_plane_linked_twice_or_with_levels_is_refused_and_a_copy_takes_no_level_slot()
    {
        for x in [f32::NAN, f32::INFINITY] {
            assert!(
                add_node_to(tree(), &new("you.skills:a", x), 256, NEAR).is_err(),
                "{x}"
            );
        }
        let twice = NewSkillNode {
            links_from: vec![SkillNodeId(1), SkillNodeId(1)],
            ..new("you.skills:a", 300.0)
        };
        assert!(add_node_to(tree(), &twice, 256, NEAR).is_err());
        let levels = NewSkillNode {
            max_level: Some(3),
            ..new("you.skills:a", 300.0)
        };
        assert!(add_node_to(tree(), &levels, 256, NEAR).is_err());
        let added = add_node_to(tree(), &new("you.skills:a", 300.0), 256, NEAR).unwrap();
        assert_eq!(
            list(&added, NODES)[2].field(LEVEL_INDEX),
            Some(&Value::Uint(0)),
            "not the model's 2"
        );
    }

    #[test]
    fn the_spacing_given_holds_however_close_earlier_mods_placed_theirs() {
        let first = add_node_to(tree(), &new("you.skills:a", 160.0), 256, NEAR).unwrap();
        assert!(
            add_node_to(first, &new("you.skills:b", 190.0), 256, NEAR).is_err(),
            "30 from the first mod's node"
        );
    }

    #[test]
    fn the_listing_names_each_node_s_kind_and_place() {
        let nodes = nodes_of(&tree());
        assert_eq!(nodes.len(), 2);
        assert_eq!(nodes[0].kind, SkillNodeType::Root);
        assert!((nodes[1].x - 100.0).abs() < f32::EPSILON);
    }
}
