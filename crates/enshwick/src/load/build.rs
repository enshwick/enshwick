use crate::abi::Str;

use super::{Load, Off, ok};

impl Load<'_> {
    /// The absolute address of a symbolic id, such as `npc.lookup`, on the
    /// running build.
    ///
    /// # Errors
    ///
    /// [`Off`] when neither the mod nor the loader pins the id for this build,
    /// or the address it names is null, so a mod that turns the address into
    /// a function pointer never makes a null one.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let lookup = load.symbol("npc.lookup")?;
    ///     assert_ne!(lookup, 0);
    ///     Ok(())
    /// }
    /// ```
    pub fn symbol(&self, name: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().symbol)(self.host.handle(), Str::new(name), &raw mut out);
        let address = ok(status, out, || format!("symbol {name}"))?;
        if address == 0 {
            return Err(Off::new(format!("symbol {name}: null on this game build")));
        }
        Ok(address)
    }

    /// An engine constant of the running build, such as a field offset that
    /// reflection does not describe.
    ///
    /// # Errors
    ///
    /// [`Off`] when the loader does not pin the constant for this build.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let row = load.constant("ecs.context_row")?;
    ///     assert_eq!(row, 8);
    ///     Ok(())
    /// }
    /// ```
    pub fn constant(&self, name: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().constant)(self.host.handle(), Str::new(name), &raw mut out);
        ok(status, out, || format!("constant {name}"))
    }

    /// The update function of an ECS system, found by its reflected name, so it
    /// survives game updates.
    ///
    /// # Errors
    ///
    /// [`Off`] when the system is missing or ambiguous on this build.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let system = load.system("add_buildzones_server")?;
    ///     assert_ne!(system, 0);
    ///     Ok(())
    /// }
    /// ```
    pub fn system(&self, name: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().system)(Str::new(name), &raw mut out);
        ok(status, out, || format!("system {name}"))
    }

    /// The offset of a field in a reflected type.
    ///
    /// # Errors
    ///
    /// [`Off`] when the type or field is missing or ambiguous on this build.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let x = load.field_offset("keen::ecs::TriggerShape", "rangeX")?;
    ///     assert_eq!(x, 4);
    ///     Ok(())
    /// }
    /// ```
    pub fn field_offset(&self, type_name: &str, field: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().field_offset)(Str::new(type_name), Str::new(field), &raw mut out);
        ok(status, out, || format!("field {type_name}.{field}"))
    }

    /// The size of a reflected type.
    ///
    /// # Errors
    ///
    /// [`Off`] when the type is missing or ambiguous on this build.
    ///
    /// # Examples
    ///
    /// ```
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let size = load.type_size("keen::ecs::TriggerShape")?;
    ///     assert!(size >= 16);
    ///     Ok(())
    /// }
    /// ```
    pub fn type_size(&self, type_name: &str) -> Result<u32, Off> {
        let mut out = 0;
        let status = (self.api().type_size)(Str::new(type_name), &raw mut out);
        ok(status, out, || format!("type {type_name}"))
    }
}
