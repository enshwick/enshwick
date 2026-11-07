//! The `[[point]]` tables of `pack.toml`: extension points a Luau mod owns,
//! laid out and hashed as `#[point]` does for a `repr(C)` struct, and the
//! schema `cargo enshwick deps` reads from them.

use serde::Deserialize;

use crate::Shape as _;
use crate::points::Mode;

/// One extension point a pack's scripts own, a `[[point]]` table of
/// `pack.toml`: other mods link into it, as into a Rust mod's `#[point]`.
///
/// # Examples
///
/// ```
/// use enshwick::packs::PackManifest;
/// use enshwick::points::Mode;
///
/// let manifest = PackManifest::parse(r#"
/// id = "you.trading-post"
/// version = "1.0.0"
/// exports = ["point:price"]
///
/// [[point]]
/// name = "price"
/// mode = "override"
/// refusable = true
/// output = "u32"
/// input = [{ name = "item", type = "enshwick.item_id" }, { name = "count", type = "u32" }]
/// "#)?;
/// let price = &manifest.point[0];
/// assert_eq!(price.mode, Mode::Override);
/// let layout = price.layout().ok_or("no layout")?;
/// assert_eq!((layout.input[1].offset, layout.size), (4, 8));
/// # Ok::<(), Box<dyn std::error::Error>>(())
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PackPoint {
    /// The local name, 1 to 128 of `[a-z0-9_./-]`: `price` is the point
    /// `<pack id>:price`.
    pub name: String,
    /// How links act on the answer.
    pub mode: Mode,
    /// Whether a link may refuse, which stops the chain.
    #[serde(default)]
    pub refusable: bool,
    /// What the point decides, for other mods' definitions.
    #[serde(default)]
    pub doc: String,
    /// The input's fields, laid out in this order without padding.
    #[serde(default)]
    pub input: Vec<PackPointField>,
    /// The answer's type, one of the field types.
    pub output: String,
}

/// One field of a [`PackPoint`]'s input.
///
/// # Examples
///
/// See [`PackPoint`].
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
#[non_exhaustive]
pub struct PackPointField {
    /// The field's name, `[a-z][a-z0-9_]*`.
    pub name: String,
    /// Its type: a number such as `u32` or `f32`, `bool`, or one of the
    /// API's ids such as `enshwick.item_id`.
    #[serde(rename = "type")]
    pub ty: String,
    /// What it holds, for other mods' definitions.
    #[serde(default)]
    pub doc: String,
}

/// A value of a [`PackPoint`] in the owner's bytes: a field of the input,
/// or the answer.
///
/// # Examples
///
/// See [`PackPoint`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PointSlot {
    /// The field's name; `output` for the answer.
    pub name: String,
    /// Its type, as `pack.toml` names it.
    pub ty: String,
    /// Where it starts.
    pub offset: usize,
    /// Its size in bytes.
    pub size: usize,
    /// Its layout hash, the one a Rust definition of the field gets.
    pub hash: u64,
}

/// A [`PackPoint`]'s input and answer, laid out.
///
/// # Examples
///
/// See [`PackPoint`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PointLayout {
    /// The input's fields, in order.
    pub input: Vec<PointSlot>,
    /// The input's size in bytes.
    pub size: u32,
    /// The answer.
    pub output: PointSlot,
}

/// A type a point's field or answer may have: its name in `pack.toml`, its
/// size, alignment and shape hash.
struct FieldType {
    name: &'static str,
    size: usize,
    align: usize,
    shape: u64,
}

/// Every type a pack point's field or answer may have, the primitives and
/// ids a script reads and writes.
const TYPES: [FieldType; 20] = [
    FieldType {
        name: "u8",
        size: 1,
        align: 1,
        shape: u8::SHAPE,
    },
    FieldType {
        name: "i8",
        size: 1,
        align: 1,
        shape: i8::SHAPE,
    },
    FieldType {
        name: "u16",
        size: 2,
        align: 2,
        shape: u16::SHAPE,
    },
    FieldType {
        name: "i16",
        size: 2,
        align: 2,
        shape: i16::SHAPE,
    },
    FieldType {
        name: "u32",
        size: 4,
        align: 4,
        shape: u32::SHAPE,
    },
    FieldType {
        name: "i32",
        size: 4,
        align: 4,
        shape: i32::SHAPE,
    },
    FieldType {
        name: "u64",
        size: 8,
        align: 8,
        shape: u64::SHAPE,
    },
    FieldType {
        name: "i64",
        size: 8,
        align: 8,
        shape: i64::SHAPE,
    },
    FieldType {
        name: "f32",
        size: 4,
        align: 4,
        shape: f32::SHAPE,
    },
    FieldType {
        name: "f64",
        size: 8,
        align: 8,
        shape: f64::SHAPE,
    },
    FieldType {
        name: "bool",
        size: 1,
        align: 1,
        shape: bool::SHAPE,
    },
    FieldType {
        name: "enshwick.guid",
        size: 16,
        align: 1,
        shape: crate::content::Guid::SHAPE,
    },
    FieldType {
        name: "enshwick.entity",
        size: 4,
        align: 4,
        shape: crate::Entity::SHAPE,
    },
    FieldType {
        name: "enshwick.item_id",
        size: 4,
        align: 4,
        shape: crate::ItemId::SHAPE,
    },
    FieldType {
        name: "enshwick.recipe_id",
        size: 4,
        align: 4,
        shape: crate::RecipeId::SHAPE,
    },
    FieldType {
        name: "enshwick.knowledge_id",
        size: 4,
        align: 4,
        shape: crate::knowledge::KnowledgeId::SHAPE,
    },
    FieldType {
        name: "enshwick.skill_node",
        size: 4,
        align: 4,
        shape: crate::abi::SkillNode::SHAPE,
    },
    FieldType {
        name: "enshwick.player_key",
        size: 8,
        align: 8,
        shape: crate::PlayerKey::SHAPE,
    },
    FieldType {
        name: "enshwick.buff_type_id",
        size: 4,
        align: 4,
        shape: crate::buffs::BuffTypeId::SHAPE,
    },
    FieldType {
        name: "enshwick.npc_id",
        size: 4,
        align: 4,
        shape: crate::survivors::NpcId::SHAPE,
    },
];

/// The words Rust or Luau keep for themselves, which no field takes, so
/// every definition of the point compiles.
const KEYWORDS: &[&str] = &[
    "abstract", "and", "as", "async", "await", "become", "box", "break", "const", "continue",
    "crate", "do", "dyn", "else", "elseif", "end", "enum", "extern", "false", "final", "fn", "for",
    "function", "gen", "if", "impl", "in", "let", "local", "loop", "macro", "match", "mod", "move",
    "mut", "nil", "not", "or", "override", "priv", "pub", "ref", "repeat", "return", "self",
    "static", "struct", "super", "then", "trait", "true", "try", "type", "typeof", "unsafe",
    "unsized", "until", "use", "virtual", "where", "while", "yield",
];

fn field_type(name: &str) -> Option<&'static FieldType> {
    TYPES.iter().find(|t| t.name == name)
}

impl PackPoint {
    /// The input and the answer laid out as `#[point]` lays out a `repr(C)`
    /// struct of these types; `None` for a point [`PackManifest::parse`]
    /// refuses.
    ///
    /// [`PackManifest::parse`]: super::PackManifest::parse
    ///
    /// # Examples
    ///
    /// See [`PackPoint`].
    #[must_use]
    pub fn layout(&self) -> Option<PointLayout> {
        self.laid_out().ok()
    }

    /// The name of the input's type in the schema: the local name in
    /// `UpperCamelCase`, `max-stack` as `MaxStack`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::PackManifest;
    ///
    /// let manifest = PackManifest::parse("id = \"you.post\"\nversion = \"1.0.0\"\n[[point]]\nname = \"max-stack\"\nmode = \"observe\"\noutput = \"u32\"\n")?;
    /// assert_eq!(manifest.point[0].input_type(), "MaxStack");
    /// # Ok::<(), enshwick::packs::PackError>(())
    /// ```
    #[must_use]
    pub fn input_type(&self) -> String {
        let mut out = String::new();
        for word in self.name.split(|c: char| !c.is_ascii_alphanumeric()) {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                out.push(first.to_ascii_uppercase());
                out.extend(chars);
            }
        }
        if !out.starts_with(|c: char| c.is_ascii_alphabetic()) {
            out.insert(0, 'P');
        }
        out
    }

    fn laid_out(&self) -> Result<PointLayout, LayOutPointError> {
        let mut input = Vec::with_capacity(self.input.len());
        let mut end = 0usize;
        let mut align = 1usize;
        for field in &self.input {
            let name = &field.name;
            let bare = field.name.starts_with(|c: char| c.is_ascii_lowercase())
                && name
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
            if !bare || KEYWORDS.contains(&name.as_str()) || name == "output" {
                return Err(LayOutPointError::Name(name.clone()));
            }
            if input.iter().any(|slot: &PointSlot| slot.name == *name) {
                return Err(LayOutPointError::Twice(name.clone()));
            }
            let ty = field_type(&field.ty).ok_or_else(|| unknown(&field.ty))?;
            if !end.is_multiple_of(ty.align) {
                return Err(LayOutPointError::Gap(name.clone()));
            }
            align = align.max(ty.align);
            input.push(PointSlot {
                name: name.clone(),
                ty: field.ty.clone(),
                offset: end,
                size: ty.size,
                hash: crate::services::layout_hash(name, ty.shape, ty.size, ty.align, end),
            });
            end += ty.size;
        }
        if !end.is_multiple_of(align) {
            return Err(LayOutPointError::GapAtEnd);
        }
        let ty = field_type(&self.output).ok_or_else(|| unknown(&self.output))?;
        let output = PointSlot {
            name: "output".to_owned(),
            ty: self.output.clone(),
            offset: 0,
            size: ty.size,
            hash: crate::services::layout_hash("output", ty.shape, ty.size, ty.align, 0),
        };
        let size = u32::try_from(end).map_err(|_| LayOutPointError::TooLarge)?;
        Ok(PointLayout {
            input,
            size,
            output,
        })
    }
}

fn unknown(name: &str) -> LayOutPointError {
    let names: Vec<&str> = TYPES.iter().map(|t| t.name).collect();
    LayOutPointError::Type(name.to_owned(), names.join(", "))
}

/// Why a point's fields do not lay out.
#[derive(Debug, thiserror::Error)]
enum LayOutPointError {
    #[error("field {0:?}: a name is [a-z][a-z0-9_]*, no keyword and not `output`")]
    Name(String),
    #[error("field {0:?} is given twice")]
    Twice(String),
    #[error("{0:?} is no type a point takes: {1}")]
    Type(String, String),
    #[error("field {0:?} would need padding before it; order the fields from the widest")]
    Gap(String),
    #[error(
        "the input would need padding at its end; order the fields from the widest, or add one"
    )]
    GapAtEnd,
    #[error("the input is too large")]
    TooLarge,
}

impl super::PackManifest {
    /// Refuses a point whose name is no local name or given twice, whose
    /// fields do not lay out, or whose input type another point names too.
    pub(super) fn check_points(&self, text: &str) -> Result<(), super::PackError> {
        let mut names = std::collections::BTreeSet::new();
        let mut types = std::collections::BTreeSet::new();
        for point in &self.point {
            let name = &point.name;
            let problem = if !crate::content::is_id(&format!("{}:{name}", self.id)) {
                format!(
                    "point {name:?}: the name is 1 to 128 of [a-z0-9_./-], without the pack's id"
                )
            } else if !names.insert(name.as_str()) {
                format!("point {name:?} is given twice")
            } else if !types.insert(point.input_type()) {
                format!(
                    "point {name:?}: its input type {} is another point's too; rename one",
                    point.input_type()
                )
            } else if let Err(why) = point.laid_out() {
                format!("point {name:?}: {why}")
            } else {
                continue;
            };
            let at = super::manifest::find(text, &format!("name = \"{name}\""));
            return Err(super::PackError::at(
                super::MANIFEST_FILE,
                text,
                at,
                &problem,
            ));
        }
        Ok(())
    }

    /// The schema of the pack's points, in the form a mod's manifest carries
    /// and `cargo enshwick deps` reads; `None` without any.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::packs::PackManifest;
    ///
    /// let manifest = PackManifest::parse("id = \"you.post\"\nversion = \"1.0.0\"\n[[point]]\nname = \"price\"\nmode = \"advise\"\noutput = \"u32\"\ninput = [{ name = \"item\", type = \"u32\" }]\n")?;
    /// let schema = manifest.points_schema().ok_or("no points")?;
    /// assert!(schema.contains("name = \"you.post:price\""));
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    #[must_use]
    pub fn points_schema(&self) -> Option<String> {
        if self.point.is_empty() {
            return None;
        }
        let mut points = Vec::new();
        let mut types = Vec::new();
        for point in &self.point {
            let layout = point.layout()?;
            let mut entry = toml::Table::new();
            entry.insert("name".into(), format!("{}:{}", self.id, point.name).into());
            entry.insert("mode".into(), mode_word(point.mode).into());
            entry.insert("refusable".into(), point.refusable.into());
            entry.insert("input".into(), point.input_type().into());
            entry.insert("doc".into(), point.doc.clone().into());
            entry.insert("output".into(), point.output.clone().into());
            entry.insert(
                "output_layout".into(),
                layout.output.hash.to_string().into(),
            );
            let hashes: Vec<toml::Value> = layout
                .input
                .iter()
                .map(|slot| slot.hash.to_string().into())
                .collect();
            entry.insert("layout".into(), hashes.into());
            points.push(toml::Value::Table(entry));
            let fields: Vec<toml::Value> = point
                .input
                .iter()
                .map(|field| {
                    let mut f = toml::Table::new();
                    f.insert("name".into(), field.name.clone().into());
                    f.insert("doc".into(), field.doc.clone().into());
                    f.insert("type".into(), field.ty.clone().into());
                    toml::Value::Table(f)
                })
                .collect();
            let mut ty = toml::Table::new();
            ty.insert("name".into(), point.input_type().into());
            ty.insert("doc".into(), point.doc.clone().into());
            ty.insert("kind".into(), "struct".into());
            ty.insert("field".into(), fields.into());
            types.push(toml::Value::Table(ty));
        }
        let mut schema = toml::Table::new();
        schema.insert(
            "schema".into(),
            i64::from(crate::schema_section::FORMAT).into(),
        );
        schema.insert("mod".into(), self.id.clone().into());
        schema.insert("version".into(), self.version.clone().into());
        schema.insert("point".into(), points.into());
        schema.insert("type".into(), types.into());
        toml::to_string(&schema).ok()
    }
}

/// The word a schema gives `mode`.
const fn mode_word(mode: Mode) -> &'static str {
    match mode {
        Mode::Observe => "observe",
        Mode::Advise => "advise",
        Mode::Override => "override",
    }
}

#[cfg(test)]
mod tests {
    use super::super::PackManifest;

    fn manifest(points: &str) -> Result<PackManifest, String> {
        PackManifest::parse(&format!("id = \"you.post\"\nversion = \"1.0.0\"\n{points}"))
            .map_err(|e| e.to_string())
    }

    #[test]
    fn a_field_that_would_need_padding_is_refused_with_its_name() {
        let error = manifest(
            "[[point]]\nname = \"price\"\nmode = \"override\"\noutput = \"u32\"\ninput = [{ name = \"cheap\", type = \"bool\" }, { name = \"item\", type = \"u32\" }]\n",
        )
        .unwrap_err();
        assert!(error.contains("\"item\" would need padding"), "{error}");
        let error = manifest(
            "[[point]]\nname = \"price\"\nmode = \"override\"\noutput = \"u32\"\ninput = [{ name = \"item\", type = \"u32\" }, { name = \"cheap\", type = \"bool\" }]\n",
        )
        .unwrap_err();
        assert!(error.contains("padding at its end"), "{error}");
    }

    #[test]
    fn an_unknown_type_a_keyword_and_a_name_twice_are_refused() {
        for (points, why) in [
            (
                "name = \"p\"\nmode = \"observe\"\noutput = \"u128\"",
                "no type a point takes",
            ),
            (
                "name = \"p\"\nmode = \"observe\"\noutput = \"u8\"\ninput = [{ name = \"end\", type = \"u8\" }]",
                "no keyword",
            ),
            (
                "name = \"p\"\nmode = \"observe\"\noutput = \"u8\"\ninput = [{ name = \"a\", type = \"u8\" }, { name = \"a\", type = \"u8\" }]",
                "given twice",
            ),
            (
                "name = \"Bad!\"\nmode = \"observe\"\noutput = \"u8\"",
                "1 to 128",
            ),
        ] {
            let error = manifest(&format!("[[point]]\n{points}\n")).unwrap_err();
            assert!(error.contains(why), "{why}: {error}");
        }
        let twice = "[[point]]\nname = \"p\"\nmode = \"observe\"\noutput = \"u8\"\n";
        let error = manifest(&format!("{twice}{twice}")).unwrap_err();
        assert!(error.contains("given twice"), "{error}");
        let error = manifest(
            "[[point]]\nname = \"max-stack\"\nmode = \"observe\"\noutput = \"u8\"\n[[point]]\nname = \"max_stack\"\nmode = \"observe\"\noutput = \"u8\"\n",
        )
        .unwrap_err();
        assert!(error.contains("another point's too"), "{error}");
    }

    #[test]
    fn the_hashes_are_the_ones_a_rust_field_of_the_type_gets() {
        use crate::Shape as _;
        let manifest = manifest(
            "[[point]]\nname = \"price\"\nmode = \"override\"\noutput = \"enshwick.item_id\"\ninput = [{ name = \"player\", type = \"enshwick.player_key\" }, { name = \"item\", type = \"enshwick.item_id\" }, { name = \"count\", type = \"u16\" }, { name = \"cheap\", type = \"bool\" }, { name = \"tier\", type = \"u8\" }]\n",
        )
        .unwrap();
        let layout = manifest.point[0].layout().unwrap();
        let hash = crate::services::layout_hash;
        let expected = [
            hash("player", crate::PlayerKey::SHAPE, 8, 8, 0),
            hash("item", crate::ItemId::SHAPE, 4, 4, 8),
            hash("count", u16::SHAPE, 2, 2, 12),
            hash("cheap", bool::SHAPE, 1, 1, 14),
            hash("tier", u8::SHAPE, 1, 1, 15),
        ];
        let got: Vec<u64> = layout.input.iter().map(|slot| slot.hash).collect();
        assert_eq!(got, expected);
        assert_eq!(layout.size, 16);
        assert_eq!(
            layout.output.hash,
            hash("output", crate::ItemId::SHAPE, 4, 4, 0)
        );
    }
}
