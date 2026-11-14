//! Reading and writing the ABI's values across the boundary: the helpers the
//! fake host shares, and the symbol lookup of a mod's build rows.

pub(crate) use enshwick_base::__private::copy::{
    copy_out, headed_in, headed_list_in, list_in, put, put_headed, put_headed_list, put_list,
    put_sized_list, room, slice, text,
};

/// The value of the symbol named `name`.
pub(crate) fn find(symbols: &[enshwick_core::abi::Symbol], name: &str) -> Option<u64> {
    symbols
        .iter()
        .find_map(|symbol| (text(symbol.name) == Some(name)).then_some(symbol.rva))
}
