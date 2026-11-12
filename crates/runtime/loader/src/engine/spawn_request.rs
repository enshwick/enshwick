//! The engine's spawn request and its starting components, laid out as the
//! build table says.

/// The largest request or component record the loader builds, far past the
/// engine's 0x30 and 0x18 bytes on server 399 and client 426.
const RECORD_MAX: usize = 0x400; // bound, not a game offset: a sanity bound

/// Where the engine's spawn request and each of its starting components keep
/// their fields.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RequestLayout {
    /// The request's size.
    pub(crate) size: usize,
    /// The entity that asks for the spawn, a `u32`.
    pub(crate) requester: usize,
    /// The template's GUID, 16 bytes.
    pub(crate) template: usize,
    /// The starting components, a pointer.
    pub(crate) initial: usize,
    /// Their count, a `u64`.
    pub(crate) initial_count: usize,
    /// One component's size, the stride of the array.
    pub(crate) component_size: usize,
    /// A component's type hash, a `u32`.
    pub(crate) component_hash: usize,
    /// Its data, a pointer.
    pub(crate) component_data: usize,
    /// Its data's size in bytes, a `u64`.
    pub(crate) component_bytes: usize,
}

/// One component a spawn starts with: its type hash, and the address and
/// size of its bytes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Start {
    pub(crate) hash: u32,
    pub(crate) data: u64,
    pub(crate) bytes: u64,
}

/// Writes `value` at `at` in `out`; `None` when it does not fit.
fn put(out: &mut [u8], at: usize, value: &[u8]) -> Option<()> {
    out.get_mut(at..at.checked_add(value.len())?)?
        .copy_from_slice(value);
    Some(())
}

/// `bytes` as 8-byte words, so the engine reads its pointers aligned.
fn words(bytes: &[u8]) -> Vec<u64> {
    bytes
        .chunks(8)
        .map(|chunk| {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            u64::from_le_bytes(word)
        })
        .collect()
}

impl RequestLayout {
    /// Whether each record stays within [`RECORD_MAX`] and every field fits
    /// inside its record, so that a request is built whole or not at all.
    pub(crate) fn fits(&self) -> bool {
        let start = Start {
            hash: 1,
            data: 1,
            bytes: 1,
        };
        self.size <= RECORD_MAX
            && self.component_size <= RECORD_MAX
            && self.components(&[start]).is_some()
            && self.request(1, [1; 16], 1, 1).is_some()
    }

    /// The array of `starts` as the engine reads it; `None` when a field
    /// falls outside a component.
    pub(crate) fn components(&self, starts: &[Start]) -> Option<Vec<u64>> {
        let mut out = vec![0u8; self.component_size.checked_mul(starts.len())?];
        for (i, start) in starts.iter().enumerate() {
            let record = out.get_mut(i * self.component_size..(i + 1) * self.component_size)?;
            put(record, self.component_hash, &start.hash.to_le_bytes())?;
            put(record, self.component_data, &start.data.to_le_bytes())?;
            put(record, self.component_bytes, &start.bytes.to_le_bytes())?;
        }
        Some(words(&out))
    }

    /// The request asked by `requester` for `template`, starting with the
    /// `count` components at `initial`; `None` when a field falls outside
    /// the request.
    pub(crate) fn request(
        &self,
        requester: u32,
        template: [u8; 16],
        initial: u64,
        count: u64,
    ) -> Option<Vec<u64>> {
        let mut out = vec![0u8; self.size];
        put(&mut out, self.requester, &requester.to_le_bytes())?;
        put(&mut out, self.template, &template)?;
        put(&mut out, self.initial, &initial.to_le_bytes())?;
        put(&mut out, self.initial_count, &count.to_le_bytes())?;
        Some(words(&out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGINE: RequestLayout = RequestLayout {
        size: 0x30,
        requester: 0,
        template: 4,
        initial: 0x18,
        initial_count: 0x20,
        component_size: 0x18,
        component_hash: 0,
        component_data: 8,
        component_bytes: 0x10,
    };

    #[test]
    fn a_request_holds_its_fields_where_the_layout_puts_them() {
        let template: [u8; 16] = core::array::from_fn(|i| u8::try_from(i + 1).unwrap());
        let words = ENGINE.request(0x42, template, 0xabc0, 3).unwrap();
        assert_eq!(words.len(), 6);
        assert_eq!(words[0], 0x0403_0201_0000_0042);
        assert_eq!(words[1], 0x0c0b_0a09_0807_0605);
        assert_eq!(words[2], 0x0000_0000_100f_0e0d);
        assert_eq!(words[3], 0xabc0);
        assert_eq!(words[4], 3);
        assert_eq!(words[5], 0);
    }

    #[test]
    fn components_follow_each_other_by_the_layout_s_stride() {
        let starts = [
            Start {
                hash: 7,
                data: 0x1000,
                bytes: 0x40,
            },
            Start {
                hash: 9,
                data: 0x2000,
                bytes: 8,
            },
        ];
        let words = ENGINE.components(&starts).unwrap();
        assert_eq!(words, [7, 0x1000, 0x40, 9, 0x2000, 8]);
        let wider = RequestLayout {
            component_size: 0x20,
            component_bytes: 0x18,
            ..ENGINE
        };
        assert_eq!(
            wider.components(&starts).unwrap(),
            [7, 0x1000, 0, 0x40, 9, 0x2000, 0, 8]
        );
    }

    #[test]
    fn the_engine_s_layout_fits() {
        assert!(ENGINE.fits());
    }

    #[test]
    fn a_layout_with_a_field_past_its_record_does_not_fit() {
        for layout in [
            RequestLayout {
                size: 0x20,
                ..ENGINE
            },
            RequestLayout {
                template: 0x24,
                ..ENGINE
            },
            RequestLayout {
                component_bytes: 0x14,
                ..ENGINE
            },
            RequestLayout {
                component_hash: usize::MAX,
                ..ENGINE
            },
            RequestLayout {
                size: RECORD_MAX + 8,
                ..ENGINE
            },
        ] {
            assert!(!layout.fits(), "{layout:x?}");
        }
    }
}
