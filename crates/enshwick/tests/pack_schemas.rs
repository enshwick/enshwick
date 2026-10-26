//! The JSON Schemas editors read for a content pack's files, held as
//! snapshots so a change to a pack's format shows in review.

#[cfg(test)]
mod tests {
    #[test]
    fn every_pack_file_has_the_schema_of_its_snapshot() {
        for (file, schema) in enshwick::packs::schemas() {
            insta::assert_snapshot!(file, schema);
        }
    }
}
