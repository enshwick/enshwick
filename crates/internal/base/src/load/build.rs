use crate::abi::Str;

use super::{BaseLoad, Off, ok};

impl BaseLoad {
    /// As `Load::symbol`.
    pub fn symbol(&self, name: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().symbol)(self.host.handle(), Str::new(name), &raw mut out);
        let address = ok(status, out, || format!("symbol {name}"))?;
        if address == 0 {
            return Err(Off::new(format!("symbol {name}: null on this game build")));
        }
        Ok(address)
    }

    /// As `Load::constant`.
    pub fn constant(&self, name: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().constant)(self.host.handle(), Str::new(name), &raw mut out);
        ok(status, out, || format!("constant {name}"))
    }

    /// As `Load::system`.
    pub fn system(&self, name: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().system)(Str::new(name), &raw mut out);
        ok(status, out, || format!("system {name}"))
    }

    /// As `Load::field_offset`.
    pub fn field_offset(&self, type_name: &str, field: &str) -> Result<u64, Off> {
        let mut out = 0;
        let status = (self.api().field_offset)(Str::new(type_name), Str::new(field), &raw mut out);
        ok(status, out, || format!("field {type_name}.{field}"))
    }

    /// As `Load::type_size`.
    pub fn type_size(&self, type_name: &str) -> Result<u32, Off> {
        let mut out = 0;
        let status = (self.api().type_size)(Str::new(type_name), &raw mut out);
        ok(status, out, || format!("type {type_name}"))
    }
}
