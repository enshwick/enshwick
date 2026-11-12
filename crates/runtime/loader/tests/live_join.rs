//! The client's join check against a running server: `ENSH_JOIN_SERVER`
//! names its query address, such as `127.0.0.1:15647`; without it the test
//! passes untested. The server runs a mod of side both, which the crafted
//! client lacks.

#[cfg(test)]
mod tests {
    #[test]
    fn a_running_server_keeps_out_a_client_without_its_mods_and_the_list_names_them() {
        let Some(address) = std::env::var_os("ENSH_JOIN_SERVER") else {
            return;
        };
        let address = address.to_string_lossy().parse().unwrap();
        let reply = enshwick_loader::base::join::query_rules(address).unwrap();
        let raw = String::from_utf8_lossy(&reply);
        assert!(
            !raw.contains("enshwick.page") && !raw.contains("enshwick.name"),
            "the server advertises no homepage or name: {raw}"
        );
        let fingerprint = enshwick_loader::base::join::rule(&reply, "enshwick").unwrap();
        let mods = enshwick_loader::base::join::mod_rules_value(&reply);
        let complete = enshwick_loader::base::join::mods_complete(&reply);
        assert!(!mods.is_empty(), "the server runs no mod of side both");
        let ours = "you.extra@1.0.0";
        let plan = enshwick_loader::base::join::plan(
            "00000000",
            ours,
            Some((&fingerprint, &mods)),
            complete,
        );
        assert!(!plan.joins());
        let found = enshwick_loader::base::join::difference(&mods, ours, "", complete);
        assert!(found.refuses());
        assert_eq!(found.extra[0].id, "you.extra");
        for row in found.rows() {
            eprintln!("{row:?}");
        }
    }
}
