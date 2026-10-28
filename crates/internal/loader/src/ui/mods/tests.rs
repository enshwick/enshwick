use super::*;

#[test]
fn a_servers_join_values_read_as_a_table() {
    let table = server_table("{ radius = 4.0, mode = \"on\" }").unwrap();
    assert_eq!(table["radius"].as_float(), Some(4.0));
    assert!(server_table("nonsense").is_none());
}
