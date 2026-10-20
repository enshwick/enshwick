//! A game executable as the pin tool reads it: its code, its function table
//! and its reflected systems.

use std::ops::Range;

use enshwick_reflect::{Fingerprint, Image, Layout, Reflection};

/// How far code is decoded past an address that no function entry covers.
const LOOSE_CODE: u64 = 0x200;

/// A game executable on disk.
pub(crate) struct Exe<'a> {
    image: Image<'a>,
    /// The `.text` bytes and the RVA they start at.
    text: &'a [u8],
    text_rva: u64,
    /// `.pdata` entries as RVA ranges, sorted by start.
    functions: Vec<Range<u64>>,
    reflection: Reflection<'a>,
}

impl<'a> Exe<'a> {
    /// Reads the executable in `bytes`.
    pub(crate) fn parse(bytes: &'a [u8]) -> Result<Self, String> {
        let image = Image::parse(bytes, Layout::File, None).map_err(|e| e.to_string())?;
        let base = image.base();
        let text_section = image.section(".text").ok_or("no .text section")?;
        let range = image.range(text_section);
        let text = image
            .read(range.start, range.end - range.start)
            .ok_or(".text is unreadable")?;
        let text_rva = range.start - base;
        let functions = image
            .section(".pdata")
            .map(|pdata| {
                let range = image.range(pdata);
                let len = (range.end - range.start).min(pdata.virtual_size());
                image
                    .read(range.start, len)
                    .map(functions)
                    .unwrap_or_default()
            })
            .unwrap_or_default();
        let reflection = Reflection::scan(&image);
        Ok(Self {
            image,
            text,
            text_rva,
            functions,
            reflection,
        })
    }

    /// The build fingerprint, when the executable carries one.
    pub(crate) fn fingerprint(&self) -> Option<Fingerprint> {
        self.image.fingerprint()
    }

    /// The `.text` bytes and the RVA of the first.
    pub(crate) fn text(&self) -> (&'a [u8], u64) {
        (self.text, self.text_rva)
    }

    /// `len` bytes at `rva`, when they lie inside `.text`.
    pub(crate) fn code(&self, rva: u64, len: u64) -> Option<&'a [u8]> {
        let start = usize::try_from(rva.checked_sub(self.text_rva)?).ok()?;
        let end = start.checked_add(usize::try_from(len).ok()?)?;
        self.text.get(start..end)
    }

    /// `len` initialised bytes at `rva`, in any section.
    pub(crate) fn data(&self, rva: u64, len: u64) -> Option<&'a [u8]> {
        self.image.read(self.image.base() + rva, len)
    }

    /// The VAs an image at the preferred base can span, for spotting absolute
    /// pointers in code: a PE image is smaller than 4 GiB.
    pub(crate) fn absolute_range(&self) -> Range<u64> {
        self.image.base()..self.image.base() + (1 << 32)
    }

    /// The function table entry that covers `rva`.
    pub(crate) fn function_at(&self, rva: u64) -> Option<Range<u64>> {
        let i = self.functions.partition_point(|f| f.start <= rva);
        let f = self.functions.get(i.checked_sub(1)?)?;
        (rva < f.end).then(|| f.clone())
    }

    /// Every function table entry, sorted by start.
    pub(crate) fn functions(&self) -> &[Range<u64>] {
        &self.functions
    }

    /// Every function table entry `len` bytes long.
    pub(crate) fn functions_of_len(&self, len: u64) -> impl Iterator<Item = &Range<u64>> {
        self.functions
            .iter()
            .filter(move |f| f.end - f.start == len)
    }

    /// The code at `rva` and how far it runs: its function table entry, or a
    /// bounded stretch when no entry covers it (a leaf function).
    pub(crate) fn code_around(&self, rva: u64) -> Option<(Range<u64>, bool)> {
        if let Some(f) = self.function_at(rva) {
            return Some((f, true));
        }
        let end = (rva + LOOSE_CODE).min(self.text_rva + self.text.len() as u64);
        (end > rva).then_some((rva..end, false))
    }

    /// The entry of the reflected system `name`, as an RVA.
    pub(crate) fn system_entry(&self, name: &str) -> Option<u64> {
        let entry = self.reflection.system(name).ok()?.entry();
        entry.checked_sub(self.image.base()).filter(|_| entry != 0)
    }
}

/// The `RUNTIME_FUNCTION` entries in `pdata`, sorted.
fn functions(pdata: &[u8]) -> Vec<Range<u64>> {
    let mut out: Vec<Range<u64>> = pdata
        .as_chunks::<12>()
        .0
        .iter()
        .filter_map(|entry| {
            let start = u32::from_le_bytes(entry.get(..4)?.try_into().ok()?);
            let end = u32::from_le_bytes(entry.get(4..8)?.try_into().ok()?);
            (start != 0 && end > start).then_some(u64::from(start)..u64::from(end))
        })
        .collect();
    out.sort_by_key(|f| f.start);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_function_table_is_read_in_order() {
        let mut pdata = Vec::new();
        for (start, end) in [(0x2000u32, 0x2010u32), (0x1000, 0x1040), (0, 0)] {
            pdata.extend_from_slice(&start.to_le_bytes());
            pdata.extend_from_slice(&end.to_le_bytes());
            pdata.extend_from_slice(&0u32.to_le_bytes());
        }
        assert_eq!(functions(&pdata), vec![0x1000..0x1040, 0x2000..0x2010]);
    }
}
