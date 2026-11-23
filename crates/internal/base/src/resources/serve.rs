//! The served resource: a builder the loader runs over a resource's base,
//! which `Load::serve` and the typed and value builders hand over, and the
//! [`Output`] it builds in.

use core::ffi::c_void;
use core::marker::PhantomData;
use core::ptr::NonNull;

use crate::Off;
use crate::abi::{ResourceKey, ResourcePatchFn, Status};

/// A served resource's builder, kept for the life of the process.
type Builder = Box<dyn Fn(&[u8], &mut Output<'_>) -> Option<()> + Send + Sync>;

/// The bytes a served resource's builder writes its resource into, which the
/// loader lends for the build, so the build copies nothing out.
///
/// It grows like a `Vec<u8>`: when it runs short, the loader lends a buffer
/// at least twice as large and the bytes move there. A builder that knows
/// its length calls [`Output::reserve`] first and builds in one buffer; one
/// that wrote fewer bytes than it holds moves them once more into an exact
/// buffer when it ends, since the game keeps that buffer. When the loader
/// lends nothing, the output is refused: later writes do nothing and the
/// build fails, as a builder's `None` does. An output with no bytes fails
/// the build too: the game reads no resource from nothing.
///
/// # Examples
///
/// ```
/// use enshwick::content::{Guid, types};
///
/// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
///
/// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
///     let chest = types::ITEM_INFO.key(CHEST)?;
///     load.serve(chest, chest, |base, out| {
///         out.reserve(base.len());
///         out.extend_from_slice(base);
///         *out.get_mut(8)? = 1;
///         Some(())
///     })
/// }
/// ```
#[derive(Debug)]
pub struct Output<'b> {
    /// The lent buffer the bytes lie in; dangling before the first loan, so
    /// reading the bytes needs no check.
    at: NonNull<u8>,
    /// Bytes lent at `at`.
    capacity: usize,
    /// Bytes written from `at`.
    len: usize,
    /// The loader lent no buffer when one was needed.
    refused: bool,
    /// Lends a buffer of the given length: [`lent`], or a test's own.
    lender: fn(usize) -> Option<*mut u8>,
    /// The build the loans belong to, which frees them when it ends.
    build: PhantomData<&'b mut [u8]>,
}

impl Output<'_> {
    /// An output with no loan yet, as a build starts it.
    pub(crate) const fn new() -> Self {
        Self::with_lender(lent)
    }

    /// An output whose loans come from `lender`.
    const fn with_lender(lender: fn(usize) -> Option<*mut u8>) -> Self {
        Self {
            at: NonNull::dangling(),
            capacity: 0,
            len: 0,
            refused: false,
            lender,
            build: PhantomData,
        }
    }

    /// Makes room for `additional` more bytes, in one loan of that size when
    /// none was taken yet, so a builder that knows its length builds without
    /// moving its bytes.
    ///
    /// # Examples
    ///
    /// See [`Output`].
    pub fn reserve(&mut self, additional: usize) {
        let Some(needed) = self.len.checked_add(additional) else {
            self.refused = true;
            return;
        };
        let wanted = if self.capacity == 0 {
            needed
        } else {
            needed.max(self.capacity.saturating_mul(2))
        };
        self.lend(needed, wanted);
    }

    /// Appends `bytes`, lending a larger buffer when this one is short.
    ///
    /// # Examples
    ///
    /// See [`Output`].
    pub fn extend_from_slice(&mut self, bytes: &[u8]) {
        let len = self.len;
        let Some(end) = len.checked_add(bytes.len()) else {
            self.refused = true;
            return;
        };
        if self.room(end) {
            self.len = end;
            self[len..].copy_from_slice(bytes);
        }
    }

    /// Makes the output a copy of `bytes`, built elsewhere; `None` when the
    /// loader lends no buffer, so a builder can end with it.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, types};
    ///
    /// const CHEST: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let chest = types::ITEM_INFO.key(CHEST)?;
    ///     load.serve(chest, chest, |base, out| out.copy_from(&base.repeat(2)))
    /// }
    /// ```
    pub fn copy_from(&mut self, bytes: &[u8]) -> Option<()> {
        self.len = 0;
        self.reserve(bytes.len());
        self.extend_from_slice(bytes);
        (!self.refused).then_some(())
    }

    /// Cuts the output to `len` bytes or grows it with zero bytes to `len`.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::content::{Guid, types};
    ///
    /// const TABLE: Guid = Guid::parse("6943053a-32c1-4718-ae22-e61427ddf54d");
    ///
    /// fn init(load: &mut enshwick::Load<'_>) -> Result<(), enshwick::Off> {
    ///     let table = types::ITEM_INFO.key(TABLE)?;
    ///     load.serve(table, table, |base, out| {
    ///         out.extend_from_slice(base);
    ///         out.resize(base.len() + 16);
    ///         Some(())
    ///     })
    /// }
    /// ```
    pub fn resize(&mut self, len: usize) {
        if len <= self.len || self.room(len) {
            let old = self.len;
            self.len = len;
            if len > old {
                self[old..].fill(0);
            }
        }
    }

    /// Whether `len` bytes fit, after a larger loan when they do not.
    fn room(&mut self, len: usize) -> bool {
        if len > self.capacity {
            self.lend(len, len.max(self.capacity.saturating_mul(2)));
        }
        !self.refused
    }

    /// Moves the bytes into a loan of `wanted` bytes, at least `needed`,
    /// when the buffer holds fewer than `needed`.
    fn lend(&mut self, needed: usize, wanted: usize) {
        if self.refused || needed <= self.capacity {
            return;
        }
        let capacity = wanted.min(u32::MAX as usize);
        if capacity < needed {
            self.refused = true;
            return;
        }
        self.move_to(capacity);
    }

    /// Moves the bytes into a new loan of `capacity >= len` bytes; refuses
    /// the output when the loader lends none.
    fn move_to(&mut self, capacity: usize) {
        let Some(at) = (self.lender)(capacity).and_then(NonNull::new) else {
            self.refused = true;
            return;
        };
        if self.len > 0 {
            // SAFETY: `self.at` holds `self.len` written bytes of an earlier
            // loan, still lent for this build, and the new loan of
            // `capacity >= self.len` bytes is apart from it.
            unsafe {
                at.as_ptr()
                    .copy_from_nonoverlapping(self.at.as_ptr(), self.len);
            }
        }
        self.at = at;
        self.capacity = capacity;
    }

    /// The loan the output lies in, exactly as long as its bytes, and their
    /// length; `None` when refused or empty.
    fn finish(mut self) -> Option<(*mut u8, u32)> {
        let len = u32::try_from(self.len)
            .ok()
            .filter(|&len| len > 0 && !self.refused)?;
        // The game's first build keeps the loan for the process, so it holds
        // no spare bytes.
        if self.capacity > self.len {
            self.move_to(self.len);
        }
        (!self.refused).then_some((self.at.as_ptr(), len))
    }
}

impl core::ops::Deref for Output<'_> {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        // SAFETY: `at` is a loan of `capacity >= len` bytes, valid while the
        // build that owns `self` runs, or dangling, aligned and non-null with
        // `len` 0 before the first loan.
        unsafe { core::slice::from_raw_parts(self.at.as_ptr(), self.len) }
    }
}

impl core::ops::DerefMut for Output<'_> {
    fn deref_mut(&mut self) -> &mut [u8] {
        // SAFETY: `at` is a loan of `capacity >= len` writable bytes, valid
        // while the build that owns `self` runs, or dangling, aligned and
        // non-null with `len` 0 before the first loan; `&mut self` makes the
        // slice the only reference to them.
        unsafe { core::slice::from_raw_parts_mut(self.at.as_ptr(), self.len) }
    }
}

impl AsRef<[u8]> for Output<'_> {
    fn as_ref(&self) -> &[u8] {
        self
    }
}

impl AsMut<[u8]> for Output<'_> {
    fn as_mut(&mut self) -> &mut [u8] {
        self
    }
}

/// Bytes that grow at their end, which a content helper builds a resource
/// in: a builder's [`Output`], to serve it with no copy, or a `Vec<u8>`.
///
/// It is sealed: `Output`, `Vec<u8>` and a `&mut` of either implement it.
/// Growth an output's loader refuses writes nothing, and the build fails.
///
/// # Examples
///
/// ```
/// use enshwick::resources::Grow;
///
/// fn twice(base: &[u8], out: &mut impl Grow) {
///     out.put(base);
///     out.put(base);
/// }
///
/// let mut bytes = Vec::new();
/// twice(&[1, 2], &mut bytes);
/// assert_eq!(bytes, [1, 2, 1, 2]);
/// ```
pub trait Grow: AsRef<[u8]> + AsMut<[u8]> + sealed::Sealed {
    /// Grows or cuts the bytes to `len`, new ones zero.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::Grow;
    ///
    /// let mut bytes = vec![7];
    /// bytes.grow(3);
    /// assert_eq!(bytes, [7, 0, 0]);
    /// ```
    fn grow(&mut self, len: usize);

    /// Appends `bytes`.
    ///
    /// # Examples
    ///
    /// See [`Grow`].
    fn put(&mut self, bytes: &[u8]);

    /// Makes room for `additional` more bytes at once, so a build that
    /// knows its final length grows in one buffer: an output's first loan
    /// is then that long and its build moves nothing.
    ///
    /// # Examples
    ///
    /// ```
    /// use enshwick::resources::Grow;
    ///
    /// let mut bytes = Vec::new();
    /// bytes.reserve(8);
    /// bytes.put(&[1, 2]);
    /// assert!(bytes.capacity() >= 8);
    /// ```
    fn reserve(&mut self, additional: usize);
}

/// Keeps [`Grow`] to the byte stores this crate knows.
pub(super) mod sealed {
    pub trait Sealed {}
    impl Sealed for Vec<u8> {}
    impl Sealed for super::Output<'_> {}
    impl<S: Sealed + ?Sized> Sealed for &mut S {}
}

impl<G: Grow + ?Sized> Grow for &mut G {
    fn grow(&mut self, len: usize) {
        (**self).grow(len);
    }

    fn put(&mut self, bytes: &[u8]) {
        (**self).put(bytes);
    }

    fn reserve(&mut self, additional: usize) {
        (**self).reserve(additional);
    }
}

impl Grow for Vec<u8> {
    fn grow(&mut self, len: usize) {
        self.resize(len, 0);
    }

    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }

    fn reserve(&mut self, additional: usize) {
        self.reserve_exact(additional);
    }
}

impl Grow for Output<'_> {
    fn grow(&mut self, len: usize) {
        self.resize(len);
    }

    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }

    fn reserve(&mut self, additional: usize) {
        Output::reserve(self, additional);
    }
}

/// Why a served resource is off once its builder panicked, as the loader
/// and the fake report it for the mod: the key of the loader's text, which
/// the loader shows in the reader's language.
#[doc(hidden)]
pub const BUILDER_PANICKED: &str = "mods.off.served.panicked";

/// The name of served resource `key` in a mod's features and off reasons,
/// as the loader and the fake give it: a registry by its name, else the
/// kind of resource and its GUID. `hash_of` answers the running build's
/// hash of a resource type's engine id.
#[doc(hidden)]
pub fn served_name(key: &ResourceKey, hash_of: impl Fn(&str) -> Option<u32>) -> String {
    use crate::engine_ids as ids;

    const WHOLE: [(&str, &str); 4] = [
        (ids::ITEM_REGISTRY, "the item registry"),
        (ids::RECIPE_REGISTRY, "the recipe registry"),
        (ids::WORKSHOP_REGISTRY, "the workshop registry"),
        (ids::UI_BUNDLE, "the UI bundle"),
    ];
    const OF: [(&str, &str); 4] = [
        (ids::TEMPLATE_COLLECTION, "template collection"),
        (ids::ITEM_INFO, "item"),
        (ids::TEMPLATE, "template"),
        (ids::UI_IMAGE, "image"),
    ];
    let is = |id: &str| hash_of(id) == Some(key.type_hash);
    if let Some((_, name)) = WHOLE.iter().find(|(id, _)| is(id)) {
        return (*name).to_owned();
    }
    let kind = OF
        .iter()
        .find(|(id, _)| is(id))
        .map_or("resource", |(_, kind)| *kind);
    format!("{kind} {}", key.guid)
}

/// Serves `key` from `base` through `builder` for the mod `host` serves,
/// as `Load::serve` does; also for a runtime that holds the mod's host
/// during its init, such as a script's.
///
/// # Errors
///
/// [`Off`] when the loader refuses it, such as after init.
#[doc(hidden)]
pub fn serve(
    host: crate::Host,
    key: ResourceKey,
    base: ResourceKey,
    builder: impl Fn(&[u8], &mut Output<'_>) -> Option<()> + Send + Sync + 'static,
) -> Result<(), Off> {
    let serve_resource = host.api().serve_resource;
    register(builder, "serve resource", |patch, user| {
        serve_resource(host.handle(), &raw const key, &raw const base, patch, user)
    })
}

/// Serves `key` from itself through `builder`, a re-key of `key` to `new`,
/// whose served read of `new` inside `builder` leaves out `new`'s own
/// re-keys, as `Load::rekey_item` does.
///
/// # Errors
///
/// [`Off`] when the loader refuses it, such as after init.
#[doc(hidden)]
pub fn rekey(
    host: crate::Host,
    key: ResourceKey,
    new: ResourceKey,
    builder: impl Fn(&[u8], &mut Output<'_>) -> Option<()> + Send + Sync + 'static,
) -> Result<(), Off> {
    let rekey_resource = host.api().rekey_resource;
    register(builder, "re-key resource", |patch, user| {
        rekey_resource(host.handle(), &raw const key, &raw const new, patch, user)
    })
}

/// Hands `builder` to the loader through `call`, as [`build`]'s user
/// pointer, which the loader keeps for the process unless it refuses it.
fn register(
    builder: impl Fn(&[u8], &mut Output<'_>) -> Option<()> + Send + Sync + 'static,
    what: &str,
    call: impl FnOnce(Option<ResourcePatchFn>, *mut c_void) -> Status,
) -> Result<(), Off> {
    let builder: Box<Builder> = Box::new(Box::new(builder));
    let user = Box::into_raw(builder);
    let patch: ResourcePatchFn = build;
    let status = call(Some(patch), user.cast());
    if !status.is_ok() {
        // SAFETY: `user` came from this function's `Box::into_raw`, and the loader
        // refused it, so it keeps no copy and this is the only owner.
        drop(unsafe { Box::from_raw(user) });
    }
    crate::load::ok(status, (), || what.to_owned())
}

/// Builds a served resource from its base for the loader.
///
/// # Safety
///
/// - `base` is null or readable for `base_len` bytes that stay unchanged for
///   the call.
/// - `user` is null or the `Box<Builder>` that [`register`] handed over for
///   [`serve`] or [`rekey`], which the loader keeps for the process.
/// - `out` and `out_len` are null or writable for one value each.
unsafe extern "system" fn build(
    base: *const u8,
    base_len: u32,
    user: *mut c_void,
    out: *mut *mut u8,
    out_len: *mut u32,
) -> Status {
    if user.is_null() || out.is_null() || out_len.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `user` is not null, as the early return ensures, and the caller
    // guarantees it is the boxed `Builder` that `register` leaked for the process,
    // so it is aligned, alive and only read through shared references.
    let builder = unsafe { &*user.cast::<Builder>() };
    let base = if base.is_null() || base_len == 0 {
        &[][..]
    } else {
        // SAFETY: `base` is not null in this branch, and the caller guarantees
        // `base_len` readable bytes there, unchanged for the call.
        unsafe { core::slice::from_raw_parts(base, base_len as usize) }
    };
    let mut output = Output::new();
    // A panic turns off only this resource: the loader runs it without this
    // builder from then on, and the mod stays on.
    let Ok(built) = crate::host::caught(|| builder(base, &mut output)) else {
        return Status::FAULTED;
    };
    let Some(()) = built else {
        return Status::REFUSED;
    };
    let Some((at, len)) = output.finish() else {
        return Status::REFUSED;
    };
    // SAFETY: `out` is not null, as the early return ensures, and the caller
    // guarantees it is writable for one pointer.
    unsafe {
        out.write(at);
    }
    // SAFETY: `out_len` is not null, as the early return ensures, and the
    // caller guarantees it is writable for one `u32`.
    unsafe {
        out_len.write(len);
    }
    Status::OK
}

/// A buffer of `len` zeroed bytes the loader lends the running patch
/// ([`HostApi::resource_buffer`](crate::abi::HostApi::resource_buffer)),
/// which it frees once no build reads them; `None` when it lends none.
fn lent(len: usize) -> Option<*mut u8> {
    let host = crate::host()?;
    let len = u32::try_from(len).ok()?;
    let mut at = core::ptr::null_mut();
    let status = (host.api().resource_buffer)(host.handle(), len, &raw mut at);
    (status.is_ok() && !at.is_null()).then_some(at)
}

#[cfg(test)]
thread_local! {
    /// The buffers [`test_lent`] lent on this thread, kept as a build keeps
    /// its loans.
    static LOANS: core::cell::RefCell<Vec<Box<[u8]>>> =
        const { core::cell::RefCell::new(Vec::new()) };
}

/// Lends `len` zero bytes, as the loader does during a build.
#[cfg(test)]
fn test_lent(len: usize) -> Option<*mut u8> {
    LOANS.with_borrow_mut(|loans| {
        loans.push(vec![0; len].into_boxed_slice());
        loans.last_mut().map(|loan| loan.as_mut_ptr())
    })
}

/// An output whose loans the test thread keeps, as a build the loader runs.
#[cfg(test)]
pub(crate) const fn test_output() -> Output<'static> {
    Output::with_lender(test_lent)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_output_reserved_a_byte_at_a_time_grows_by_doubling() {
        let mut output = Output::with_lender(test_lent);
        let bytes: Vec<u8> = (0..=u8::MAX).cycle().take(1000).collect();
        for byte in &bytes {
            output.reserve(1);
            output.extend_from_slice(&[*byte]);
        }
        assert_eq!(*output, bytes[..]);
        let loans = LOANS.with_borrow(Vec::len);
        assert!(loans <= 11, "{loans} loans for 1000 bytes");
    }

    #[test]
    fn a_finished_output_lies_in_a_loan_exactly_as_long_as_its_bytes() {
        let mut output = Output::with_lender(test_lent);
        output.reserve(4);
        output.extend_from_slice(&[1, 2, 3, 4, 5]);
        let (at, len) = output.finish().unwrap();
        assert_eq!(len, 5);
        LOANS.with_borrow(|loans| {
            let kept = loans.iter().find(|loan| loan.as_ptr() == at.cast_const());
            assert_eq!(kept.map(|loan| &loan[..]), Some(&[1, 2, 3, 4, 5][..]));
        });
    }

    #[test]
    fn an_output_with_no_bytes_fails_its_build() {
        let mut output = Output::with_lender(test_lent);
        output.reserve(16);
        assert!(output.finish().is_none());
    }

    #[test]
    fn a_builder_reads_a_null_base_as_empty_and_refuses_without_a_lent_buffer() {
        static SAW_EMPTY: core::sync::atomic::AtomicBool =
            core::sync::atomic::AtomicBool::new(false);
        let builder: Builder = Box::new(|base: &[u8], out: &mut Output<'_>| {
            SAW_EMPTY.store(base.is_empty(), core::sync::atomic::Ordering::Relaxed);
            out.extend_from_slice(&[1]);
            Some(())
        });
        let user = core::ptr::from_ref(&builder).cast_mut().cast();
        let (mut out, mut out_len) = (core::ptr::null_mut::<u8>(), 0u32);
        // SAFETY: a null base, `user` points at the `Builder` alive in this
        // frame, and both out pointers are writable locals.
        let status = unsafe { build(core::ptr::null(), 0, user, &raw mut out, &raw mut out_len) };
        assert!(SAW_EMPTY.load(core::sync::atomic::Ordering::Relaxed));
        assert_eq!(status, Status::REFUSED, "no host lends a buffer here");
        assert!(out.is_null());
        // SAFETY: `user` points at the `Builder` alive in this frame, and
        // `build` refuses a null `out` before it writes anything.
        let refused = unsafe {
            build(
                core::ptr::null(),
                0,
                user,
                core::ptr::null_mut(),
                &raw mut out_len,
            )
        };
        assert_eq!(refused, Status::BAD_ARGS);
    }

    #[test]
    fn an_output_without_a_loan_ignores_writes_and_refuses() {
        let mut output = Output::new();
        output.extend_from_slice(&[1, 2, 3]);
        output.resize(8);
        assert!(output.is_empty(), "no host lends a buffer here");
        assert!(output.finish().is_none());
    }

    #[test]
    fn a_panicking_builder_answers_faulted_and_writes_nothing() {
        /// A panic payload whose own drop panics again.
        struct Bomb;
        impl Drop for Bomb {
            fn drop(&mut self) {
                panic!("the payload's own drop");
            }
        }
        let builder: Builder = Box::new(|_: &[u8], _: &mut Output<'_>| std::panic::panic_any(Bomb));
        let user = core::ptr::from_ref(&builder).cast_mut().cast();
        let (mut out, mut out_len) = (core::ptr::null_mut::<u8>(), 0u32);
        // SAFETY: a null base, `user` points at the `Builder` alive in this
        // frame, and both out pointers are writable locals.
        let status = unsafe { build(core::ptr::null(), 0, user, &raw mut out, &raw mut out_len) };
        assert_eq!(status, Status::FAULTED);
        assert!(out.is_null());
        assert_eq!(out_len, 0);
    }
}
