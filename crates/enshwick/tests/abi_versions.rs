//! A mod built for an older ABI minor starts on the table of a loader of a
//! newer one, and on no table of another major.

#[cfg(test)]
mod tests {
    use enshwick::abi::{ABI_MAJOR, ABI_MINOR, HostApi, Status};
    use enshwick::test::{FakeHost, Installed};

    struct Older;

    impl enshwick::Mod for Older {
        fn init(_: &mut enshwick::Load<'_>) -> Result<Self, enshwick::Off> {
            Ok(Self)
        }
    }

    enshwick::test::declare_mod!(
        Older,
        r#"
        id = "you.older-minor"
        side = "server"
        "#
    );

    /// The fake's table as a loader of ABI `major`.`minor` writes it, with a
    /// function appended that this build does not know.
    #[repr(C)]
    struct Later {
        api: HostApi,
        appended: usize,
    }

    /// `loader`'s table, restated as a loader of ABI `major`.`minor` that
    /// appended one function writes it, for the rest of the process.
    fn table(loader: &Installed, major: u16, minor: u16) -> &'static HostApi {
        // SAFETY: the fake's table is a static, aligned and never written.
        let mut api = unsafe { *loader.api() };
        api.header.abi_major = major;
        api.header.abi_minor = minor;
        api.header.size = u32::try_from(size_of::<Later>()).unwrap();
        &Box::leak(Box::new(Later { api, appended: 0 })).api
    }

    #[test]
    fn a_mod_of_an_older_minor_starts_on_a_newer_loader_and_not_on_another_major() {
        let loader = FakeHost::new().install();
        // SAFETY: `api` is a table of the fake's functions that lives for the
        // process, and the handle is the fake's.
        let init = |api: &'static HostApi| unsafe { enshwick_mod_init(api, loader.handle()) };
        assert_eq!(init(table(&loader, ABI_MAJOR + 1, 0)), Status::BAD_ARGS);
        assert!(Older::instance().is_none());
        assert_eq!(init(table(&loader, ABI_MAJOR, ABI_MINOR + 1)), Status::OK);
        assert!(Older::instance().is_some());
    }
}
