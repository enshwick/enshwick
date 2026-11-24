//! The buffers the loader owns for patch outputs, each followed by zeroed
//! bytes the game may read past a served resource's end.

use core::mem::ManuallyDrop;
use core::ptr::NonNull;

#[cfg(test)]
thread_local! {
    /// How many buffers are alive on this thread, kept ones included.
    pub(super) static LIVE: core::cell::Cell<usize> = const { core::cell::Cell::new(0) };
}

/// How many zeroed bytes a [`Buffer`] of `len` bytes keeps after them: a
/// page, or a sixteenth of `len` where that is more. The game reads some
/// served resources past their end, harmlessly in its own archive pool; past
/// an exactly sized block it faults on a page the heap has not committed. On
/// client 26430625 its box copy (0x1410af160) sweeps a 548 x 244
/// `VoxelTemperatureResource` part of 83 layers and 7,280 bytes in 4 x 4 x 1
/// boxes through the whole 84th layer, 126,432 bytes past the end; a
/// sixteenth covers a whole layer of any volume of 16 layers or more.
pub(super) fn tail(len: usize) -> usize {
    (len / 16).max(PAGE)
}

/// The size of a page of memory on Windows x64.
pub(super) const PAGE: usize = 4096;

/// `len` zeroed bytes with room for a [`Buffer`]'s tail after them, so
/// [`Buffer::new`] takes them without a copy.
pub(super) fn zeroed(len: usize) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(len.saturating_add(tail(len)));
    bytes.resize(len, 0);
    bytes
}

/// Bytes the loader owns for a patch's output, freed on drop unless kept:
/// the buffer's `len` bytes, then [`tail`] zeroed ones no output reaches.
pub(super) struct Buffer {
    /// The whole allocation, the tail included.
    pub(super) all: NonNull<[u8]>,
    len: usize,
}

impl Buffer {
    /// `bytes` with their tail of zeros after them.
    pub(super) fn new(mut bytes: Vec<u8>) -> Self {
        let len = bytes.len();
        bytes.reserve_exact(tail(len));
        bytes.resize(len + tail(len), 0);
        #[cfg(test)]
        LIVE.set(LIVE.get() + 1);
        Self {
            all: NonNull::from(Box::leak(bytes.into_boxed_slice())),
            len,
        }
    }

    pub(super) fn at(&self) -> usize {
        self.all.cast::<u8>().as_ptr().expose_provenance()
    }

    fn bytes(&self) -> &[u8] {
        // SAFETY: `self.all` came from `Box::leak` in `new` and lives until
        // `self` goes; the patches that wrote it have returned.
        let all = unsafe { self.all.as_ref() };
        &all[..self.len]
    }

    /// Whether the `len` bytes at `data` lie in the buffer's own bytes.
    pub(super) fn holds(&self, data: usize, len: usize) -> bool {
        data >= self.at()
            && data
                .checked_add(len)
                .is_some_and(|end| end <= self.at() + self.len)
    }

    /// Leaves the bytes for the process: the game holds them.
    pub(super) fn keep(self) {
        let _kept = ManuallyDrop::new(self);
    }

    fn into_box(self) -> Box<[u8]> {
        #[cfg(test)]
        LIVE.set(LIVE.get() - 1);
        let this = ManuallyDrop::new(self);
        // SAFETY: `this.all` came from `Box::leak` in `new`, and `ManuallyDrop`
        // keeps `drop` from freeing it again.
        unsafe { Box::from_raw(this.all.as_ptr()) }
    }
}

impl Drop for Buffer {
    fn drop(&mut self) {
        #[cfg(test)]
        LIVE.set(LIVE.get() - 1);
        // SAFETY: `self.all` came from `Box::leak` in `new`, and only this drop,
        // `keep` and `into_box` take it, each once.
        drop(unsafe { Box::from_raw(self.all.as_ptr()) });
    }
}

/// The bytes of a finished build and the loader's buffer they lie in, freed
/// with them unless kept.
pub(super) struct Lent {
    pub(super) data: usize,
    pub(super) owned: Buffer,
}

impl Lent {
    /// Leaves the `len` bytes for the process with zeros from their end to
    /// the tail, so the game reads zeros past them however much of their
    /// buffer they fill.
    pub(super) fn keep(self, len: usize) {
        let Self { data, mut owned } = self;
        let end = data - owned.at() + len;
        // SAFETY: `owned` came from `Box::leak` in `Buffer::new` and is the
        // build's alone once its patches have returned; `leave` found the
        // `len` bytes at `data` in its bytes, so `end` is at most its `len`.
        let all = unsafe { owned.all.as_mut() };
        all[end..owned.len].fill(0);
        owned.keep();
    }

    /// The `len` bytes, taken over without a copy when they fill their
    /// buffer.
    pub(super) fn into_bytes(self, len: usize) -> Vec<u8> {
        let start = self.data - self.owned.at();
        if start == 0 && self.owned.len == len {
            let mut bytes = self.owned.into_box().into_vec();
            bytes.truncate(len);
            return bytes;
        }
        self.owned.bytes()[start..start + len].to_vec()
    }
}
