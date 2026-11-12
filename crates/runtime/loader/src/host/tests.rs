use super::join::{SHOWN_VALUES, shown};

#[test]
fn a_server_s_join_values_stay_on_one_log_line() {
    assert_eq!(shown("{ a = \"Сейф\" }"), "{ a = \"Сейф\" }");
    assert_eq!(
        shown("{ a = 1 }\n12:00:00.000Z ERROR [enshwick] forged"),
        "{ a = 1 }\\n12:00:00.000Z ERROR [enshwick] forged"
    );
    let long = shown(&"x".repeat(SHOWN_VALUES * 4));
    assert_eq!(long.len(), SHOWN_VALUES + 3);
}
