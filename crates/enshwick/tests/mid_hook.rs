//! A mod's mid-function hook against the fake loader: registered, run on the
//! registers, and a panic that leaves them whole.

#[cfg(test)]
mod tests {
    use enshwick::abi::Registers;
    use enshwick::abi::Status;
    use enshwick::test::FakeHost;

    const SITE: u64 = 0x1_4009_0a6c;

    struct Doubler;

    impl enshwick::Mod for Doubler {
        fn manifest() -> &'static enshwick::abi::Manifest {
            enshwick::manifest!(id: "doubler", side: SERVER)
        }

        fn init(load: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            let site = load.symbol("site")?;
            // SAFETY: the fake runs the handler on its own registers.
            unsafe {
                load.hook_at(site, |registers| {
                    registers.rdx = 1;
                    assert_ne!(registers.rcx, 13, "unlucky");
                    registers.rax = registers.rcx * 2;
                })
            }?;
            Ok(Self)
        }
    }

    enshwick::declare_mod!(Doubler);

    #[test]
    fn a_mid_function_hook_writes_registers_and_a_panic_leaves_them_whole() {
        let loader = FakeHost::new().symbol("site", SITE).install();
        // SAFETY: `enshwick_mod_init` is the export of this file's `declare_mod!(Doubler)`.
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
