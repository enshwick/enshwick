//! The typed API as a script reaches it: a handle per role, opened by the
//! generated type's id at start, whose values cross as tables by the
//! generated items' Rust names, read and written through the generated
//! Rust type itself.

use super::{RawComponent, RawEvent, RawResource, Row};
use crate::content::Guid;
use crate::resources::{Registry, Resources, Value as Data};
use crate::script::{Answer, Class, Error, FromScript, IntoScript, Ty, Typed, Value};
use crate::world::World;
use crate::{Entity, Host, Load, Off};

/// A generated type's value in a script: a table by the generated item's
/// field names, shaped by its record.
#[derive(Clone, Debug)]
pub(crate) struct Fields(Value);

impl Typed for Fields {
    const TY: &'static Ty = &Ty::Data;
}

impl IntoScript for Fields {
    fn into_value(self) -> Value {
        self.0
    }
}

impl FromScript for Fields {
    fn from_value(value: Value) -> Result<Self, Error> {
        Ok(Self(value))
    }
}

impl Answer for Result<Fields, Error> {
    const TY: &'static Ty = &Ty::Data;

    fn from_answer(answer: Result<Value, Error>) -> Self {
        answer.map(Fields)
    }
}

/// `value`, of `ty`, with each record a table by its fields' names, as a
/// script holds game data.
fn by_name(value: Value, ty: &Ty) -> Value {
    match (value, ty) {
        (Value::Record(values), Ty::Record(record)) => Value::Map(
            record
                .fields()
                .iter()
                .zip(values)
                .map(|(field, value)| (field.name().to_owned(), by_name(value, field.ty())))
                .collect(),
        ),
        (Value::List(items), Ty::List(inner)) => {
            Value::List(items.into_iter().map(|item| by_name(item, inner)).collect())
        }
        (value, Ty::Optional(inner)) => by_name(value, inner),
        // A generated enum's `Other` holds plain game data, nothing to rename.
        (value, _) => value,
    }
}

/// `value`, a script's game data of `ty`, with each table of a record in
/// its fields' order; a field the table leaves out is nil, and a key no
/// field has is refused.
fn by_place(value: Value, ty: &Ty) -> Result<Value, Error> {
    match (value, ty) {
        (Value::Map(mut entries), Ty::Record(record)) => {
            let fields = record
                .fields()
                .iter()
                .map(|field| {
                    let at = entries.iter().position(|(name, _)| name == field.name());
                    match at {
                        Some(at) => by_place(entries.swap_remove(at).1, field.ty())
                            .map_err(|e| e.within(field.name())),
                        None => Ok(Value::Nil),
                    }
                })
                .collect::<Result<_, _>>()?;
            if let Some((name, _)) = entries.first() {
                return Err(Error::new(format!(
                    "{name:?} is no field of {}",
                    record.name()
                )));
            }
            Ok(Value::Record(fields))
        }
        // An empty table reads as a list: a record without fields, such as
        // a base, written `{}`.
        (Value::List(items), Ty::Record(_)) if items.is_empty() => {
            by_place(Value::Map(Vec::new()), ty)
        }
        (Value::List(items), Ty::List(inner)) => items
            .into_iter()
            .enumerate()
            .map(|(i, item)| by_place(item, inner).map_err(|e| e.within(&(i + 1).to_string())))
            .collect::<Result<_, _>>()
            .map(Value::List),
        (Value::Nil, Ty::Optional(_)) => Ok(Value::Nil),
        (value, Ty::Optional(inner)) => by_place(value, inner),
        (value, _) => Ok(value),
    }
}

impl Row {
    /// The generated type of id `id`, its parts joined by `.` or `::`, with
    /// the role `role`.
    fn find(id: &str, role: &str) -> Result<&'static Self, Off> {
        let id = id.replace('.', "::");
        crate::typed::NAMED
            .iter()
            .find(|row| row.id == id && row.roles.contains(&role))
            .ok_or_else(|| {
                Off::new(format!(
                    "typed {id}: no generated {} of that id",
                    role.to_lowercase()
                ))
            })
    }

    /// The codec's `value` as a script gets it, read through the generated
    /// type.
    fn shown(&self, value: &Data) -> Result<Fields, Off> {
        let value =
            (self.to_script)(value).map_err(|e| Off::new(format!("typed {}: {e}", self.id)))?;
        Ok(Fields(by_name(value, self.script)))
    }

    /// A script's `fields` as the codec writes them, read through the
    /// generated type.
    fn taken(&self, fields: Fields) -> Result<Data, Off> {
        by_place(fields.0, self.script)
            .and_then(self.from_script)
            .map_err(|e| Off::new(format!("typed {}: {e}", self.id)))
    }

    /// `value` as a script gets it and back as the codec writes it, the
    /// path the script handles take, for the tests that hold every
    /// generated type to it.
    ///
    /// # Errors
    ///
    /// [`Off`] when a step refuses the value.
    #[doc(hidden)]
    pub fn through_script(&self, value: &Data) -> Result<Data, Off> {
        self.taken(self.shown(value)?)
    }

    /// The generated component of id `id`, its parts joined by `.` or `::`.
    pub(crate) fn component(id: &str) -> Result<&'static Self, Off> {
        Self::find(id, "Component")
    }

    /// A script's table of every field of the type, as the codec writes it.
    pub(crate) fn value_of(&self, fields: Value) -> Result<Data, Off> {
        self.taken(Fields(fields))
    }

    /// `value` with the fields `fields` names replaced, the others kept: a
    /// script's partial table over a value of the type.
    pub(crate) fn overlaid(&self, value: &Data, fields: &Value) -> Result<Data, Off> {
        let Fields(Value::Map(mut current)) = self.shown(value)? else {
            return Err(Off::new(format!("typed {}: not a struct", self.id)));
        };
        let Value::Map(changes) = fields else {
            return Err(Off::new(format!(
                "typed {}: the fields to change are a table by field name",
                self.id
            )));
        };
        for (name, change) in changes {
            match current.iter_mut().find(|(field, _)| field == name) {
                Some((_, held)) => held.clone_from(change),
                None => {
                    return Err(Off::new(format!(
                        "typed {}: {name:?} is no field of it",
                        self.id
                    )));
                }
            }
        }
        self.taken(Fields(Value::Map(current)))
    }
}

/// A generated component type on the running build, as a script holds it.
#[derive(Clone)]
pub(crate) struct ComponentType {
    row: &'static Row,
    raw: RawComponent,
    /// The world, or why the build lacks it: only reads and writes need it.
    world: Result<World, Off>,
}

impl ComponentType {
    fn world(&self) -> Result<World, Off> {
        self.world
            .clone()
            .map_err(|why| Off::new(format!("typed {}: {why}", self.row.id)))
    }
}

#[crate::script(reads)]
impl ComponentType {
    /// `entity`'s component as a table by the generated struct's field
    /// names, on the game thread; nil when the entity has none, or off the
    /// game thread.
    ///
    /// # Errors
    ///
    /// [`Off`] when the component's bytes do not read as the generated
    /// struct, or on a build that lacks the world's reads.
    #[script]
    pub(crate) fn read(&self, entity: Entity) -> Result<Option<Fields>, Off> {
        self.raw
            .read(self.world()?, entity)?
            .map(|value| self.row.shown(&value))
            .transpose()
    }

    /// Writes `value`, a table with every field of the generated struct,
    /// to `entity`'s component, marked changed the game's way, keeping the
    /// bytes of what the struct does not hold. On the game thread only.
    ///
    /// # Errors
    ///
    /// [`Off`] for a type the loader does not let mods write, a value that
    /// does not fit the type, off the game thread, or on a build that lacks
    /// the world's reads.
    #[script]
    pub(crate) fn write(&self, entity: Entity, value: Fields) -> Result<(), Off> {
        self.raw
            .write(self.world()?, entity, &self.row.taken(value)?)
    }
}

impl crate::script::Object for ComponentType {
    const CLASS: &'static Class = &Class::of::<Self>(
        "ComponentType",
        "A generated component type on the running build: entities' components read and written as tables.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_READS];
}

/// A generated game event type on the running build, as a script holds it.
#[derive(Clone)]
pub(crate) struct EventType {
    row: &'static Row,
    raw: RawEvent,
    host: Host,
}

#[crate::script(handlers)]
impl EventType {
    /// Calls `handler` with each event of the type the game raises, as a
    /// table by the generated struct's field names, on the server's game
    /// thread; an event that does not read is logged once and skipped.
    /// Subscribe at the top level of main.luau.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader refuses the handler, such as after the start.
    #[script]
    pub(crate) fn on(&self, handler: impl Fn(Fields) + Send + Sync + 'static) -> Result<(), Off> {
        let row = self.row;
        self.raw.on(self.host, move |value| {
            handler(row.shown(value)?);
            Ok(())
        })
    }
}

impl crate::script::Object for EventType {
    const CLASS: &'static Class = &Class::of::<Self>(
        "EventType",
        "A generated game event type on the running build: its events handled as tables.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_HANDLERS];
}

/// A generated resource type on the running build, as a script holds it.
#[derive(Clone)]
pub(crate) struct ResourceType {
    row: &'static Row,
    raw: RawResource,
    host: Host,
}

#[crate::script(values)]
impl ResourceType {
    /// The key of the resource of this type with GUID `guid`, part 0, as
    /// `{ guid, type, part }`.
    ///
    /// # Errors
    ///
    /// [`Off`] outside the game.
    #[script]
    pub(crate) fn key(&self, guid: Guid) -> Result<Data, Off> {
        Ok(Resources::key_data(Registry::game()?, self.raw.key(guid)))
    }

    /// The game's own resource `key`, `{ guid, type, part }`, as a table by
    /// the generated struct's field names, read from its archive.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed key or one of another type, a key the archive
    /// lacks, and a resource whose bytes do not read as the generated
    /// struct.
    #[script]
    pub(crate) fn game_value(&self, key: &Data) -> Result<Fields, Off> {
        self.row
            .shown(&self.raw.game_value(Resources::key_of(key)?)?)
    }

    /// Serves the resource `key`, made from the resource `base` the first
    /// time the game reads it: `change` gets the base's value as a table by
    /// the generated struct's field names and returns the value to serve,
    /// every field given, or nil to serve the base unchanged; the bytes of
    /// what the struct does not hold stay as the base has them. A change
    /// that fails or does not fit the type is logged, and the base is
    /// served unchanged. Allowed while the mod starts.
    ///
    /// # Errors
    ///
    /// [`Off`] for a malformed key, a key or base of another type, and when
    /// the loader refuses the serve, such as after the start.
    #[script]
    pub(crate) fn serve(
        &self,
        key: &Data,
        base: &Data,
        change: impl Fn(Fields) -> Result<Fields, Error> + Send + Sync + 'static,
    ) -> Result<(), Off> {
        let row = self.row;
        let (key, base) = (Resources::key_of(key)?, Resources::key_of(base)?);
        self.raw.serve(self.host, key, base, move |value| {
            match change(row.shown(&value)?)? {
                Fields(Value::Nil) => Ok(None),
                changed => row.taken(changed).map(Some),
            }
        })
    }
}

impl crate::script::Object for ResourceType {
    const CLASS: &'static Class = &Class::of::<Self>(
        "ResourceType",
        "A generated resource type on the running build: its resources read and served as tables.",
    );
    const PARTS: &'static [&'static [crate::script::Function<Self>]] = &[Self::SCRIPT_VALUES];
}

#[crate::script(typed)]
impl Load<'_> {
    /// The generated component type `id`, its path in the typed API joined
    /// by dots, such as `ecs.Health`, on the running build, its shape
    /// checked.
    ///
    /// # Errors
    ///
    /// [`Off`] for an id of no generated component, and when the build
    /// lacks the type or its shape differs from the generated struct,
    /// naming the build.
    #[script(name = "component_type")]
    pub(crate) fn typed_component_type(&mut self, id: &str) -> Result<ComponentType, Off> {
        let row = Row::find(id, "Component")?;
        Ok(ComponentType {
            row,
            raw: RawComponent::resolve(self, row.id, row.check)?,
            world: self.world(),
        })
    }

    /// The generated game event type `id`, its path in the typed API
    /// joined by dots, such as `ecs.EntityDiedEvent`, on the running build,
    /// its shape checked.
    ///
    /// # Errors
    ///
    /// [`Off`] for an id of no generated event, and when the build lacks
    /// the type or its shape differs from the generated struct, naming the
    /// build.
    #[script(name = "event_type")]
    pub(crate) fn typed_event_type(&mut self, id: &str) -> Result<EventType, Off> {
        let row = Row::find(id, "Event")?;
        Ok(EventType {
            row,
            raw: RawEvent::resolve(self, row.id, row.check)?,
            host: self.host(),
        })
    }

    /// The generated resource type `id`, its path in the typed API joined
    /// by dots, such as `ItemCollectionResource`, on the running build, its
    /// shape checked.
    ///
    /// # Errors
    ///
    /// [`Off`] for an id of no generated resource, and when the build lacks
    /// the type or its shape differs from the generated struct, naming the
    /// build.
    #[script(name = "resource_type")]
    pub(crate) fn typed_resource_type(&mut self, id: &str) -> Result<ResourceType, Off> {
        let row = Row::find(id, "Resource")?;
        Ok(ResourceType {
            row,
            raw: RawResource::resolve(self, row.id, row.check)?,
            host: self.host(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::typed::runtime::{reflected_enum, reflected_struct};

    reflected_enum! {
        /// A test enum.
        Mood {
            /// Calm.
            Calm = "Calm",
        }
    }

    reflected_struct! {
        /// A test struct.
        Inner {
            /// A level.
            level: u8 = "level",
        }
    }

    reflected_struct! {
        /// A test struct holding the others.
        Outer: Inner {
            /// A mood.
            mood: Mood = "mood",
            /// Some inner values.
            inners: Vec<Inner> = "inners",
            /// A count past what a script number holds.
            r#type: u64 = "type",
            /// An inner value or none.
            spare: Option<Box<Inner>> = "spare",
        }
    }

    fn outer() -> Outer {
        Outer {
            base: Inner { level: 1 },
            mood: Mood::Other(crate::typed::Constant::Name("Wild".into())),
            inners: vec![Inner { level: 2 }],
            r#type: u64::MAX,
            spare: None,
        }
    }

    #[test]
    fn a_value_crosses_by_its_rust_names_and_comes_back_whole() {
        let shown = by_name(outer().into_value(), Outer::TY);
        let Value::Map(fields) = &shown else {
            panic!("{shown:?}")
        };
        let names: Vec<&str> = fields.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["base", "mood", "inners", "type", "spare"]);
        assert!(matches!(&fields[3].1, Value::String(text) if *text == u64::MAX.to_string()));
        let Value::Map(mood) = &fields[1].1 else {
            panic!("{:?}", fields[1].1)
        };
        assert!(
            matches!(&mood[0], (other, Value::String(name)) if other == "Other" && name == "Wild")
        );
        let back = <Outer as FromScript>::from_value(by_place(shown, Outer::TY).unwrap()).unwrap();
        assert_eq!(back, outer());
    }

    #[test]
    fn a_table_with_a_key_no_field_has_is_refused() {
        let Value::Map(mut fields) = by_name(outer().into_value(), Outer::TY) else {
            unreachable!()
        };
        fields.push(("rank".into(), Value::Number(3.0)));
        let error = by_place(Value::Map(fields), Outer::TY).unwrap_err();
        assert!(
            error.to_string().contains("\"rank\" is no field"),
            "{error}"
        );
        let inner = |extra: bool| {
            let mut fields = vec![("level".to_owned(), Value::Number(1.0))];
            if extra {
                fields.push(("rank".to_owned(), Value::Number(2.0)));
            }
            Value::Map(fields)
        };
        let list = Value::List(vec![inner(false), inner(true)]);
        let error = by_place(list, <Vec<Inner> as Typed>::TY).unwrap_err();
        assert!(
            error.to_string().starts_with("2: "),
            "counted from 1: {error}"
        );
    }

    #[test]
    fn an_empty_table_reads_as_a_record_without_fields() {
        assert!(matches!(
            by_place(Value::List(Vec::new()), Empty::TY),
            Ok(Value::Record(fields)) if fields.is_empty()
        ));
    }

    reflected_struct! {
        /// A test struct without fields.
        Empty {}
    }

    #[test]
    fn a_variant_name_reads_and_a_wrong_one_names_the_way_out() {
        assert_eq!(
            <Mood as FromScript>::from_value(Value::String("Calm".into())).unwrap(),
            Mood::Calm
        );
        let error = <Mood as FromScript>::from_value(Value::String("Angry".into())).unwrap_err();
        assert!(error.to_string().contains("Other"), "{error}");
        assert!(
            error.to_string().contains("tests::Mood"),
            "names the enum: {error}"
        );
        let numbered = Value::Map(vec![("Other".into(), Value::Number(4.0))]);
        assert_eq!(
            <Mood as FromScript>::from_value(numbered).unwrap(),
            Mood::Other(crate::typed::Constant::Number(4))
        );
    }

    #[test]
    fn an_other_that_names_a_variant_reads_as_that_variant() {
        let named = Value::Map(vec![("Other".into(), Value::String("Calm".into()))]);
        assert_eq!(<Mood as FromScript>::from_value(named).unwrap(), Mood::Calm);
    }

    #[test]
    fn a_64_bit_field_takes_a_whole_number_a_script_computed() {
        let Value::Map(mut fields) = by_name(outer().into_value(), Outer::TY) else {
            unreachable!()
        };
        fields[3].1 = Value::Number(42.0);
        let back =
            <Outer as FromScript>::from_value(by_place(Value::Map(fields), Outer::TY).unwrap())
                .unwrap();
        assert_eq!(back.r#type, 42);
        assert_eq!(i64::from_value(Value::Number(-7.0)).unwrap(), -7);
        for refused in [-1.0, 0.5, 2f64.powi(54)] {
            assert!(
                u64::from_value(Value::Number(refused)).is_err(),
                "{refused}"
            );
        }
    }

    #[test]
    fn a_record_is_named_by_its_rust_path_and_a_raw_field_by_its_plain_name() {
        let Ty::Record(record) = Outer::TY else {
            panic!("{:?}", Outer::TY)
        };
        assert!(
            record
                .name()
                .ends_with("typed::runtime::script::tests::Outer")
        );
        assert_eq!(record.fields()[3].name(), "type");
    }
}
