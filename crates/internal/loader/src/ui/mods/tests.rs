use super::*;

#[test]
fn a_page_past_a_shrunk_list_moves_to_its_last_page() {
    assert_eq!(clamp_first(8, 20, 8), 8);
    assert_eq!(clamp_first(16, 12, 8), 8);
    assert_eq!(clamp_first(8, 5, 8), 0);
    assert_eq!(clamp_first(8, 8, 8), 0);
    assert_eq!(clamp_first(8, 0, 8), 0);
}

#[test]
fn a_servers_join_values_read_as_a_table() {
    let table = server_table("{ radius = 4.0, mode = \"on\" }").unwrap();
    assert_eq!(table["radius"].as_float(), Some(4.0));
    assert!(server_table("nonsense").is_none());
}
