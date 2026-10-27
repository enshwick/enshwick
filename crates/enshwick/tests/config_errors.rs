//! Why the settings check refuses a text, told without the text's values.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::test::{FakeHost, Installed};
    use enshwick::{Load, Mod, Off};

    /// How the vault opens.
    #[derive(Default, serde::Deserialize, serde::Serialize)]
    #[serde(rename_all = "lowercase")]
    enum Mode {
        /// On its own.
        #[default]
        Auto,
        /// Only by hand.
        Manual,
    }

    /// Vault settings.
    #[derive(Default, serde::Deserialize, serde::Serialize, enshwick::Config)]
    struct Settings {
        /// The code that opens the vault.
        pin: u32,
        /// How it opens.
        mode: Mode,
        /// The vault's key.
        #[config(secret)]
        token: String,
    }

    struct Vault;

    impl Mod for Vault {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.vault", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            let _settings: Settings = load.config()?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Vault);

    fn started() -> Installed {
        let loader = FakeHost::new()
            .settings("pin = 1\nmode = \"auto\"\ntoken = \"x\"\n")
            .install();
        // SAFETY: both functions are the exports of this file's `declare_mod!(Vault)`.
        let status = unsafe { loader.start(enshwick_mod_init, enshwick_mod_ready) };
        assert_eq!(status, Status::OK);
        loader
    }

    /// The check's reason for `text`, asserted to hold nothing of `secret`.
    fn refused(loader: &Installed, text: &str, secret: &str) -> String {
        let reason = loader.check_settings(text).unwrap_err().reason;
        assert!(!reason.contains(secret), "{reason}");
        reason
    }

    #[test]
    fn a_refused_value_is_named_by_line_key_and_type_only() {
        let loader = started();
        assert_eq!(
            refused(
                &loader,
                "mode = \"auto\"\npin = \"hunter2\"\ntoken = \"x\"\n",
                "hunter2"
            ),
            "line 2, key pin: expected u32"
        );
        assert_eq!(
            refused(
                &loader,
                "pin = 1\nmode = \"hunter2\"\ntoken = \"x\"\n",
                "hunter2"
            ),
            "line 2, key mode: expected `auto` or `manual`"
        );
        assert_eq!(
            refused(
                &loader,
                "pin = 1\nmode = \"auto\"\ntoken = 4815162342\n",
                "4815162342"
            ),
            "line 3, key token: expected a string"
        );
        let broken = refused(&loader, "pin = 1\ntoken = \"hunter2\n", "hunter2");
        assert!(broken.starts_with("line 2: "), "{broken}");
    }

    proptest::proptest! {
        #[test]
        fn no_value_reaches_the_checks_reason(
            word in "zq[a-z]{6}",
            number in 5_000_000_000u64..9_999_999_999,
            shape in 0usize..6,
        ) {
            let loader = started();
            let (text, secret) = match shape {
                0 => (format!("pin = \"{word}\"\nmode = \"auto\"\ntoken = \"x\"\n"), word),
                1 => (format!("pin = 1\nmode = \"{word}\"\ntoken = \"x\"\n"), word),
                2 => (format!("pin = 1\nmode = \"auto\"\ntoken = {number}\n"), number.to_string()),
                3 => (format!("pin = {number}\nmode = \"auto\"\ntoken = \"x\"\n"), number.to_string()),
                4 => (format!("pin = 1\nmode = \"auto\"\ntoken = \"{word}\n"), word),
                _ => (format!("pin = [\"{word}\"]\nmode = \"auto\"\ntoken = \"x\"\n"), word),
            };
            refused(&loader, &text, &secret);
        }
    }
}
