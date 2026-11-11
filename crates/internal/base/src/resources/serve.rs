//! The served resource: a builder the loader runs over a resource's base,
//! which `Load::serve` and the typed and value builders hand over.

use core::ffi::c_void;

use crate::Off;
use crate::abi::{ResourceKey, ResourcePatchFn, Status};

/// A served resource's builder, kept for the life of the process.
type Builder = Box<dyn Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync>;

/// Serves `key` from `base` through `builder` for the mod `host` serves,
/// as `Load::serve` does; also for a runtime that holds the mod's host
/// during its init, such as a script's.
///
/// # Errors
///
/// [`Off`] when the loader refuses it, such as after init.
pub fn serve(
    host: crate::Host,
    key: ResourceKey,
    base: ResourceKey,
    builder: impl Fn(&[u8]) -> Option<Vec<u8>> + Send + Sync + 'static,
) -> Result<(), Off> {
    let api = host.api();
    let builder: Box<Builder> = Box::new(Box::new(builder));
    let user = Box::into_raw(builder);
    let patch: ResourcePatchFn = build;
    let status = (api.serve_resource)(
        host.handle(),
        &raw const key,
        &raw const base,
        patch,
        user.cast(),
    );
    if !status.is_ok() {
        // SAFETY: `user` came from this function's `Box::into_raw`, and the loader
        // refused it, so it keeps no copy and this is the only owner.
        drop(unsafe { Box::from_raw(user) });
    }
    crate::load::ok(status, (), || "serve resource".to_owned())
}

/// Builds a served resource from its base for the loader.
///
/// # Safety
///
/// - `base` is null or readable for `base_len` bytes that stay unchanged for
///   the call.
/// - `user` is null or the `Box<Builder>` that [`serve`] handed over, which
///   the loader keeps for the process.
/// - `out` and `out_len` are null or writable for one value each.
unsafe extern "system" fn build(
    base: *const u8,
    base_len: usize,
    user: *mut c_void,
    out: *mut *const u8,
    out_len: *mut usize,
) -> Status {
    if user.is_null() || out.is_null() || out_len.is_null() {
        return Status::BAD_ARGS;
    }
    // SAFETY: `user` is not null, as the early return ensures, and the caller
    // guarantees it is the boxed `Builder` that `serve` leaked for the process,
    // so it is aligned, alive and only read through shared references.
    let builder = unsafe { &*user.cast::<Builder>() };
    let base = if base.is_null() || base_len == 0 {
        &[][..]
    } else {
        // SAFETY: `base` is not null in this branch, and the caller guarantees
        // `base_len` readable bytes there, unchanged for the call.
        unsafe { core::slice::from_raw_parts(base, base_len) }
    };
    let built = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| builder(base)));
    let Ok(Some(bytes)) = built else {
        return Status::REFUSED;
    };
    let len = bytes.len();
    // Bytes the loader cannot lend a buffer for stay for the process, as
    // `ResourcePatchFn` allows: they are handed over, never read here again.
    let at = lent(&bytes)
        .unwrap_or_else(|| Box::into_raw(bytes.into_boxed_slice()).cast::<u8>())
        .cast_const();
    // SAFETY: `out` is not null, as the early return ensures, and the caller
    // guarantees it is writable for one pointer.
    unsafe {
        out.write(at);
    }
    // SAFETY: `out_len` is not null, as the early return ensures, and the
    // caller guarantees it is writable for one `usize`.
    unsafe {
        out_len.write(len);
    }
    Status::OK
}

/// `bytes` copied into a buffer the loader lends the running patch
/// ([`HostApi::resource_buffer`](crate::abi::HostApi::resource_buffer)),
/// which it frees once no build reads them; `None` when it lends none.
fn lent(bytes: &[u8]) -> Option<*mut u8> {
    let host = crate::host()?;
    let len = u32::try_from(bytes.len()).ok()?;
    let mut at = core::ptr::null_mut();
    let status = (host.api().resource_buffer)(host.handle(), len, &raw mut at);
    if !status.is_ok() || at.is_null() {
        return None;
    }
    // SAFETY: the loader lent `len` writable bytes at `at`, apart from
    // `bytes`, for the build that runs this patch.
    unsafe { at.copy_from_nonoverlapping(bytes.as_ptr(), bytes.len()) };
    Some(at)
}
