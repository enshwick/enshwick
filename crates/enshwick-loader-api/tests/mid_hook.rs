//! A mod's mid-function hook against the fake loader: registered, run on the
//! registers, and a panic that leaves them whole.

#[cfg(test)]
mod tests {
    use enshwick_loader_api::Registers;
    use enshwick_loader_api::abi::Status;
    use enshwick_loader_api::test::FakeHost;

    const SITE: u64 = 0x1_4009_0a6c;

    struct Doubler;

    impl enshwick_loader_api::Mod for Doubler {
        fn manifest() -> &'static enshwick_loader_api::abi::Manifest {
            enshwick_loader_api::manifest!(id: "doubler", side: SERVER)
        }

        fn init(
            load: &mut enshwick_loader_api::Load<'_>,
        ) -> Result<Self, enshwick_loader_api::Off> {
            load.hook_at(load.symbol("site")?, |registers| {
                registers.rdx = 1;
                assert_ne!(registers.rcx, 13, "unlucky");
                registers.rax = registers.rcx * 2;
            })?;
            Ok(Self)
        }
    }

    enshwick_loader_api::declare_mod!(Doubler);

    #[test]
    fn a_mid_function_hook_writes_registers_and_a_panic_leaves_them_whole() {
        let loader = FakeHost::new().symbol("site", SITE).install();
        // SAFETY: the export of `declare_mod!` above.
        let status = unsafe { loader.init(enshwick_mod_init) };
        assert_eq!(status, Status::OK);
        assert_eq!(loader.mid_hooks(), [SITE]);

        let mut registers = Registers {
            rcx: 21,
            ..Registers::default()
        };
        assert_eq!(loader.run_at(SITE, &mut registers), 1);
        assert_eq!((registers.rax, registers.rdx), (42, 1));

        let mut unlucky = Registers {
            rcx: 13,
            ..Registers::default()
        };
        assert_eq!(loader.run_at(SITE, &mut unlucky), 1);
        assert_eq!(
            unlucky,
            Registers {
                rcx: 13,
                ..Registers::default()
            }
        );
        assert_eq!(loader.faults(), ["panic in the hook at 0x140090a6c"]);
    }
}
