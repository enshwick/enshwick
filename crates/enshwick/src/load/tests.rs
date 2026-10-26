use super::with_server;

/// Test settings.
#[derive(Debug, Default, serde::Deserialize, serde::Serialize, crate::Config)]
#[config(validate = Settings::check)]
struct Settings {
    /// A join value.
    #[config(sync)]
    table: Vec<u8>,
    /// This side's own.
    local: u8,
}

impl Settings {
    fn check(&self) -> Result<(), String> {
        (self.table.len() < 4)
            .then_some(())
            .ok_or_else(|| "a long table".to_owned())
    }
}

#[test]
fn a_failed_call_names_what_failed_and_why() {
    use crate::abi::Status;
    assert_eq!(super::ok(Status::OK, 3, || unreachable!()), Ok(3));
    let off = super::ok(Status::MISSING, (), || "component type keen::X".to_owned());
    assert_eq!(
        off.unwrap_err().to_string(),
        "component type keen::X: missing on this game build"
    );
}

#[test]
fn the_server_values_apply_over_the_file_and_are_checked() {
    let mut settings = Settings {
        table: vec![1],
        local: 5,
    };
    with_server(&mut settings, "{ table = [2, 4] }").unwrap();
    assert_eq!(
        (settings.table.as_slice(), settings.local),
        (&[2, 4][..], 5)
    );
    assert!(with_server(&mut settings, "{ table = [1, 2, 3, 4] }").is_err());
    assert_eq!(settings.table, [2, 4], "a refused value leaves them");
    assert!(with_server(&mut settings, "{ table = 'x' }").is_err());
    assert_eq!(settings.table, [2, 4]);
}
