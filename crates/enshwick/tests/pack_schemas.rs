//! The JSON Schemas editors read for a data pack's files, held as
//! snapshots so a change to a pack's format shows in review. Descriptions,
//! which come from the docs, are left out: only the shape is held.

#[cfg(test)]
mod tests {
    use serde_json::Value;

    /// `schema` without its descriptions, at every depth; a property named
    /// `description` stays, since its value is a schema, not text.
    fn shape(mut schema: Value) -> Value {
        fn strip(value: &mut Value) {
            match value {
                Value::Object(map) => {
                    if map.get("description").is_some_and(Value::is_string) {
                        map.remove("description");
                    }
                    map.values_mut().for_each(strip);
                }
                Value::Array(items) => items.iter_mut().for_each(strip),
                _ => {}
            }
        }
        strip(&mut schema);
        schema
    }

    #[test]
    fn every_pack_file_has_the_schema_shape_of_its_snapshot() {
        for (file, schema) in enshwick::packs::schemas() {
            let schema: Value = serde_json::from_str(&schema).unwrap();
            let shape = serde_json::to_string_pretty(&shape(schema)).unwrap();
            insta::assert_snapshot!(file, shape);
        }
    }
}
