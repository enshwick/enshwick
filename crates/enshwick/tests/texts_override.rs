//! A server's texts file the mod cannot read turns the mod off rather than
//! passing silently.

#[cfg(test)]
mod tests {
    use enshwick::abi::Status;
    use enshwick::{Load, Mod, Off};

    struct Texty;

    impl Mod for Texty {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "you.texty", side: SERVER)
        }

        fn init(load: &mut Load<'_>) -> Result<Self, Off> {
            load.texts("[en]\n\"locker.name\" = \"Locker\"\n")?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Texty);

    #[test]
    fn an_override_saved_as_utf16_turns_the_mod_off() {
        let game = std::env::temp_dir().join(format!("enshwick-texts-{}", std::process::id()));
        let configs = game.join("mod-configs");
        std::fs::create_dir_all(&configs).unwrap();
        let utf16: Vec<u8> = "[en]\n\"locker.name\" = \"Шафка\"\n"
            .encode_utf16()
            .flat_map(u16::to_le_bytes)
            .collect();
        std::fs::write(
            configs.join("texty.texts.toml"),
            [&[0xff, 0xfe][..], &utf16].concat(),
        )
        .unwrap();
        let data = game.join("mod-data").join("texty");
        let loader = enshwick::test::FakeHost::new()
            .data_dir(data.to_str().unwrap())
            .install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Texty)`.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_eq!(status, Status::REFUSED);
        assert!(
            loader
                .logs()
                .iter()
                .any(|(_, line)| line.contains("texty.texts.toml")),
            "{:?}",
            loader.logs()
        );
        let _gone = std::fs::remove_dir_all(&game);
    }
}
