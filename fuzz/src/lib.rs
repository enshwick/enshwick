//! Helpers the fuzz targets share.

#![forbid(unsafe_code)]

/// Splits fuzz input into fields at `0xff`, a byte no UTF-8 text holds, so a
/// seed is plain text fields joined by that byte.
///
/// The last field takes the rest, separators included; `None` when there are
/// fewer than `N` fields.
///
/// # Examples
///
/// ```
/// use enshwick_fuzz::fields;
///
/// assert_eq!(fields::<2>(b"ab\xffcd\xffef"), Some([&b"ab"[..], &b"cd\xffef"[..]]));
/// assert_eq!(fields::<3>(b"ab"), None);
/// ```
#[must_use]
pub fn fields<const N: usize>(data: &[u8]) -> Option<[&[u8]; N]> {
    let mut parts = data.splitn(N, |&b| b == SEPARATOR);
    let mut out = [&[][..]; N];
    for slot in &mut out {
        *slot = parts.next()?;
    }
    Some(out)
}

/// [`fields`] as text; `None` when a field is not UTF-8.
///
/// # Examples
///
/// ```
/// use enshwick_fuzz::text_fields;
///
/// assert_eq!(text_fields::<2>(b"ab\xffcd"), Some(["ab", "cd"]));
/// ```
#[must_use]
pub fn text_fields<const N: usize>(data: &[u8]) -> Option<[&str; N]> {
    let raw = fields::<N>(data)?;
    let mut out = [""; N];
    for (slot, bytes) in out.iter_mut().zip(raw) {
        *slot = core::str::from_utf8(bytes).ok()?;
    }
    Some(out)
}

/// The byte between fields.
pub const SEPARATOR: u8 = 0xff;
