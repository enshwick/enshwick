//! A generated type's row as a script reaches it.
//!
//! Its values cross as tables by the generated items' Rust names, read and
//! written through the type's [`Shape`](super::Shape), which takes and
//! refuses what the generated Rust type does.

use super::{Role, Row};
use crate::Off;
use crate::resources::Value as Data;
use crate::script::Value;
use crate::script::convert::Fields;
#[cfg(any(test, feature = "test"))]
use crate::script::{Error, Ty};

/// `value`, of `ty`, with each record a table by its fields' names, as a
/// script holds game data: the compiled path the tests hold every shape to.
#[cfg(any(test, feature = "test"))]
#[must_use]
pub fn by_name(value: Value, ty: &Ty) -> Value {
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
/// field has is refused; the other half of [`by_name`].
#[cfg(any(test, feature = "test"))]
pub fn by_place(value: Value, ty: &Ty) -> Result<Value, Error> {
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
    pub fn find(id: &str, role: Role) -> Result<&'static Self, Off> {
        let id = id.replace('.', "::");
        crate::typed::NAMED
            .iter()
            .find(|row| row.id == id && row.roles.contains(&role))
            .ok_or_else(|| {
                Off::new(format!(
                    "typed {id}: no generated {} of that id",
                    role.noun()
                ))
            })
    }

    /// The codec's `value` as a script gets it, read through the type's
    /// shape.
    pub fn shown(&self, value: &Data) -> Result<Fields, Off> {
        self.shape
            .shown(value)
            .map(Fields)
            .map_err(|e| Off::new(format!("typed {}: {e}", self.id)))
    }

    /// A script's `fields` as the codec writes them, read through the
    /// type's shape.
    pub fn taken(&self, fields: Fields) -> Result<Data, Off> {
        self.shape
            .taken(fields.0)
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
    pub fn component(id: &str) -> Result<&'static Self, Off> {
        Self::find(id, Role::Component)
    }

    /// A script's table of every field of the type, as the codec writes it.
    pub fn value_of(&self, fields: Value) -> Result<Data, Off> {
        self.taken(Fields(fields))
    }

    /// `value` with the fields `fields` names replaced, the others kept: a
    /// script's partial table over a value of the type.
    pub fn overlaid(&self, value: &Data, fields: &Value) -> Result<Data, Off> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::Guid;
    use crate::script::{FromScript, IntoScript, Typed};
    use crate::typed::runtime::{Compiled, Shaped, reflected_enum, reflected_struct};

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
                .ends_with("typed::runtime::rows::tests::Outer")
        );
        assert_eq!(record.fields()[3].name(), "type");
    }

    reflected_struct! {
        /// A test struct with a field of every leaf kind and a bitmask.
        Leaves {
            /// A small signed number.
            small: i8 = "small",
            /// A wider unsigned one.
            wide: u16 = "wide",
            /// A switch.
            flag: bool = "flag",
            /// A ratio.
            ratio: f32 = "ratio",
            /// A name.
            name: String = "name",
            /// A reference.
            id: Guid = "id",
            /// Untyped data.
            raw: Data = "raw",
            /// A variant or none.
            held: Option<crate::resources::Variant> = "held",
            /// Moods as flags.
            moods: crate::typed::Flags<Mood> = "moods",
        }
    }

    fn leaves() -> Data {
        Data::Struct(vec![
            ("small".into(), Data::Int(-3)),
            ("wide".into(), Data::Uint(700)),
            ("flag".into(), Data::Uint(2)),
            ("ratio".into(), Data::Float(0.1)),
            ("name".into(), Data::Bytes(b"wolf".to_vec())),
            ("id".into(), Data::Guid(Guid([7; 16]))),
            ("raw".into(), Data::List(vec![Data::Uint(u64::MAX)])),
            (
                "held".into(),
                Data::Variant(Box::new(crate::resources::Variant::new(
                    "demo::Sphere",
                    Data::Float(1.5),
                ))),
            ),
            ("moods".into(), Data::Flags(vec!["Calm".into()])),
        ])
    }

    /// `value` with the field `name` set to `to`.
    fn with(value: &Data, name: &str, to: Data) -> Data {
        let mut value = value.clone();
        if let Some(held) = value.field_mut(name) {
            *held = to;
        }
        value
    }

    /// The shape of `T` reads `data` and takes back `script` as its
    /// compiled conversions do, refusals by the same words.
    fn same<T>(data: &Data, script: Value)
    where
        T: Shaped + crate::typed::Reflected + Typed + IntoScript + FromScript,
    {
        let compiled = Compiled::of::<T>();
        let shape = T::SHAPE;
        assert_eq!(
            format!("{:?}", shape.shown(data)),
            format!("{:?}", (compiled.to_script)(data)),
            "{data:?}"
        );
        let shown = format!("{script:?}");
        assert_eq!(
            format!(
                "{:?}",
                shape.taken(script.clone()).map_err(|e| e.to_string())
            ),
            format!(
                "{:?}",
                (compiled.from_script)(script).map_err(|e| e.to_string())
            ),
            "{shown}"
        );
    }

    #[test]
    fn a_shape_reads_and_refuses_the_codec_s_values_as_the_rust_type_does() {
        let value = crate::typed::Reflected::to_value(&outer());
        let shown = by_name(outer().into_value(), Outer::TY);
        same::<Outer>(&value, shown);
        let leaves = leaves();
        let leaves_shown = Leaves::SHAPE.shown(&leaves).unwrap();
        same::<Leaves>(&leaves, leaves_shown);
        for (name, wrong) in [
            ("small", Data::Int(-200)),
            ("small", Data::Uint(5)),
            ("wide", Data::Int(-1)),
            ("wide", Data::Float(1.0)),
            ("flag", Data::Text("yes".into())),
            ("ratio", Data::Int(1)),
            ("name", Data::Uint(1)),
            (
                "id",
                Data::Text("6943053a-32c1-4718-ae22-e61427ddf54d".into()),
            ),
            ("held", Data::Nil),
            ("held", Data::Int(1)),
            ("moods", Data::Uint(0b101)),
            ("moods", Data::Flags(vec!["Wild".into()])),
            ("moods", Data::Name("Calm".into())),
        ] {
            same::<Leaves>(&with(&leaves, name, wrong), Value::Nil);
        }
        for (name, wrong) in [
            ("mood", Data::Name("Wild".into())),
            ("mood", Data::Uint(9)),
            ("mood", Data::Text("Calm".into())),
            ("inners", Data::Bytes(vec![0, 5, 0, 0])),
            ("inners", Data::List(vec![Data::Struct(Vec::new())])),
            ("type", Data::Int(-1)),
            ("spare", Data::Struct(vec![("level".into(), Data::Uint(1))])),
            ("spare", Data::Uint(1)),
        ] {
            same::<Outer>(&with(&value, name, wrong), Value::Nil);
        }
        assert!(Leaves::SHAPE.check(&leaves).is_ok());
        let wrong = with(&leaves, "small", Data::Int(-200));
        assert_eq!(
            Leaves::SHAPE.check(&wrong).map_err(|e| e.to_string()),
            crate::typed::check::<Leaves>(&wrong).map_err(|e| e.to_string())
        );
        assert!(Leaves::SHAPE.check(&wrong).is_err());
        same::<Outer>(&Data::Struct(Vec::new()), Value::Nil);
        same::<Outer>(&Data::Uint(1), Value::Nil);
        same::<Empty>(&Data::Uint(1), Value::List(Vec::new()));
    }

    #[test]
    fn a_shape_takes_and_refuses_a_script_s_tables_as_the_rust_type_does() {
        let value = crate::typed::Reflected::to_value(&outer());
        let Value::Map(fields) = by_name(outer().into_value(), Outer::TY) else {
            unreachable!()
        };
        let set = |name: &str, to: Value| {
            let mut fields = fields.clone();
            match fields.iter_mut().find(|(field, _)| field == name) {
                Some((_, held)) => *held = to,
                None => fields.push((name.to_owned(), to)),
            }
            Value::Map(fields)
        };
        let inner = |level: Value| Value::Map(vec![("level".into(), level)]);
        for script in [
            set("mood", Value::String("Calm".into())),
            set("mood", Value::String("Angry".into())),
            set(
                "mood",
                Value::Map(vec![("Other".into(), Value::String("Calm".into()))]),
            ),
            set(
                "mood",
                Value::Map(vec![("Other".into(), Value::Number(4.0))]),
            ),
            set("mood", Value::Number(1.0)),
            set("type", Value::Number(42.0)),
            set("type", Value::String("x".into())),
            set("type", Value::Number(2f64.powi(54))),
            set("inners", Value::List(vec![inner(Value::Number(300.0))])),
            set("inners", Value::List(vec![inner(Value::Nil)])),
            set("inners", Value::Map(Vec::new())),
            set("spare", inner(Value::Number(1.0))),
            set("spare", Value::String("none".into())),
            set("base", Value::List(Vec::new())),
            set("base", Value::Nil),
            set("rank", Value::Number(1.0)),
            Value::Map(Vec::new()),
            Value::String("outer".into()),
            Value::List(vec![Value::Nil]),
        ] {
            same::<Outer>(&value, script);
        }
        let leaves = leaves();
        let Ok(Value::Map(fields)) = Leaves::SHAPE.shown(&leaves) else {
            unreachable!()
        };
        let set = |name: &str, to: Value| {
            let mut fields = fields.clone();
            if let Some((_, held)) = fields.iter_mut().find(|(field, _)| field == name) {
                *held = to;
            }
            Value::Map(fields)
        };
        for script in [
            set("small", Value::Number(-129.0)),
            set("wide", Value::Number(1.5)),
            set("flag", Value::Number(1.0)),
            set("ratio", Value::String("0.1".into())),
            set("name", Value::Number(1.0)),
            set("id", Value::String("not a guid".into())),
            set("raw", Value::Vector([0.0; 3])),
            set(
                "held",
                Value::Map(vec![("value".into(), Value::Number(1.0))]),
            ),
            set("held", Value::Nil),
            set("moods", Value::List(vec![Value::String("Calm".into())])),
            set(
                "moods",
                Value::List(vec![Value::Map(vec![("Other".into(), Value::Number(3.0))])]),
            ),
            set(
                "moods",
                Value::List(vec![
                    Value::String("Calm".into()),
                    Value::Map(vec![("Other".into(), Value::Number(3.0))]),
                ]),
            ),
            set("moods", Value::List(vec![Value::String("Angry".into())])),
            set("moods", Value::String("Calm".into())),
        ] {
            same::<Leaves>(&leaves, script);
        }
    }
}
