//! Whether a newer Enshwick release exists: asked of GitHub once per start on
//! a thread of its own, shown on the Mods screen, logged on every side.

use semver::Version;

pub use enshwick_gamedata::releases::Release;
#[cfg(windows)]
use enshwick_gamedata::releases::{LIMIT, LOADER_TAGS, newer, path};

/// The running loader's version.
///
/// # Panics
///
/// Never: the crate's own version is valid SemVer.
#[must_use]
pub fn running() -> Version {
    #[expect(
        clippy::expect_used,
        reason = "Cargo checks the crate's version is SemVer"
    )]
    Version::parse(env!("CARGO_PKG_VERSION")).expect("the crate version is SemVer")
}

/// Asks GitHub for the releases on a thread of its own and keeps a newer one
/// in `loader` for the Mods screen; the answer, or why there is none, is
/// logged once.
#[cfg(windows)]
pub(crate) fn start(loader: &'static crate::host::Loader) {
    let spawned = std::thread::Builder::new()
        .name("enshwick-releases".into())
        .spawn(move || {
            let running = running();
            match fetch::answer(path(&running)) {
                Ok(answer) => match newer(&answer, &running, LOADER_TAGS) {
                    Ok(Some(release)) => {
                        crate::log!(
                            INFO,
                            "Enshwick {} is available, this is {running}: {}",
                            release.version,
                            release.page()
                        );
                        let _first = loader.newer_release.set(release);
                    }
                    Ok(None) => crate::log!(INFO, "Enshwick {running} is the newest release"),
                    Err(e) => {
                        crate::log!(INFO, "release check: GitHub's answer does not read: {e}");
                    }
                },
                Err(e) => crate::log!(INFO, "release check: {e}"),
            }
        });
    if let Err(e) = spawned {
        crate::log!(INFO, "release check: no thread for it: {e}");
    }
}

#[cfg(windows)]
mod fetch {
    use core::ffi::c_void;
    use core::ptr::{null, null_mut};

    use windows_sys::Win32::Networking::WinHttp::{
        INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
        WINHTTP_QUERY_FLAG_NUMBER, WINHTTP_QUERY_STATUS_CODE, WinHttpCloseHandle, WinHttpConnect,
        WinHttpOpen, WinHttpOpenRequest, WinHttpQueryHeaders, WinHttpReadData,
        WinHttpReceiveResponse, WinHttpSendRequest, WinHttpSetTimeouts,
    };
    use windows_sys::core::{PCWSTR, w};

    /// Each step's limit, resolving, connecting, sending and receiving, in
    /// milliseconds: the check gives up rather than wait on a slow network.
    const TIMEOUT: i32 = 5_000;
    /// Bytes read per call.
    const CHUNK: usize = 16 * 1024;

    /// Why GitHub's answer did not arrive.
    #[derive(Debug, thiserror::Error)]
    pub(super) enum FetchReleasesError {
        /// A Windows HTTP call failed: no network, no answer in time, or TLS.
        #[error("{call} failed: {source}")]
        Call {
            call: &'static str,
            source: std::io::Error,
        },
        /// GitHub answered with another status than 200.
        #[error("GitHub answered with HTTP status {0}")]
        Status(u32),
        /// The answer was longer than the check reads.
        #[error("GitHub's answer is longer than {} bytes", super::LIMIT)]
        TooLong,
    }

    /// A Windows HTTP handle, closed when dropped.
    struct Handle(*mut c_void);

    impl Handle {
        /// `raw` from a Windows HTTP open call, or the failure of `call`.
        fn new(raw: *mut c_void, call: &'static str) -> Result<Self, FetchReleasesError> {
            if raw.is_null() {
                Err(failed(call))
            } else {
                Ok(Self(raw))
            }
        }
    }

    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: a handle a WinHTTP open call returned, closed only here.
            let _closed = unsafe { WinHttpCloseHandle(self.0) };
        }
    }

    /// The failure of `call` with the thread's last Windows error.
    fn failed(call: &'static str) -> FetchReleasesError {
        let source = std::io::Error::last_os_error();
        FetchReleasesError::Call { call, source }
    }

    /// `ok`, a Windows HTTP `BOOL`, as the failure of `call` when it is false.
    fn check(ok: i32, call: &'static str) -> Result<(), FetchReleasesError> {
        if ok == 0 { Err(failed(call)) } else { Ok(()) }
    }

    /// GitHub's JSON answer to a GET of `path`, over HTTPS with the system's
    /// proxy settings.
    pub(super) fn answer(path: &str) -> Result<Vec<u8>, FetchReleasesError> {
        let path: Vec<u16> = path.encode_utf16().chain([0]).collect();
        let agent: PCWSTR = w!("enshwick (https://github.com/enshwick)");
        // SAFETY: `agent` is NUL-terminated; null proxy names go with the
        // automatic proxy type.
        let session = unsafe {
            WinHttpOpen(
                agent,
                WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
                null(),
                null(),
                0,
            )
        };
        let session = Handle::new(session, "WinHttpOpen")?;
        // SAFETY: `session` is an open session handle.
        let timeouts = unsafe { WinHttpSetTimeouts(session.0, TIMEOUT, TIMEOUT, TIMEOUT, TIMEOUT) };
        check(timeouts, "WinHttpSetTimeouts")?;
        // SAFETY: an open session and a NUL-terminated host name.
        let connect = unsafe {
            WinHttpConnect(
                session.0,
                w!("api.github.com"),
                INTERNET_DEFAULT_HTTPS_PORT,
                0,
            )
        };
        let connect = Handle::new(connect, "WinHttpConnect")?;
        // SAFETY: an open connection and NUL-terminated verb and path; null
        // version, referrer and accept types take WinHTTP's defaults.
        let request = unsafe {
            WinHttpOpenRequest(
                connect.0,
                w!("GET"),
                path.as_ptr(),
                null(),
                null(),
                null(),
                WINHTTP_FLAG_SECURE,
            )
        };
        let request = Handle::new(request, "WinHttpOpenRequest")?;
        let headers = w!("Accept: application/vnd.github+json\r\nX-GitHub-Api-Version: 2022-11-28");
        // SAFETY: an open request; `headers` is NUL-terminated, which the
        // length `u32::MAX` (-1) tells WinHTTP; no body.
        let sent = unsafe { WinHttpSendRequest(request.0, headers, u32::MAX, null(), 0, 0, 0) };
        check(sent, "WinHttpSendRequest")?;
        // SAFETY: an open request that was sent; the reserved pointer is null.
        let received = unsafe { WinHttpReceiveResponse(request.0, null_mut()) };
        check(received, "WinHttpReceiveResponse")?;
        let mut status = 0u32;
        let mut size = u32::try_from(size_of::<u32>()).unwrap_or(4);
        // SAFETY: a received request; with the number flag WinHTTP writes one
        // `u32` to `status`, whose size `size` holds.
        let queried = unsafe {
            WinHttpQueryHeaders(
                request.0,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                null(),
                (&raw mut status).cast(),
                &raw mut size,
                null_mut(),
            )
        };
        check(queried, "WinHttpQueryHeaders")?;
        if status != 200 {
            return Err(FetchReleasesError::Status(status));
        }
        let mut body = Vec::new();
        let mut chunk = vec![0u8; CHUNK];
        loop {
            let mut read = 0u32;
            // SAFETY: a received request; `chunk` is writable for `CHUNK`
            // bytes and `read` takes how many were written.
            let ok = unsafe {
                WinHttpReadData(
                    request.0,
                    chunk.as_mut_ptr().cast(),
                    u32::try_from(CHUNK).unwrap_or(u32::MAX),
                    &raw mut read,
                )
            };
            check(ok, "WinHttpReadData")?;
            let Some(got) = chunk.get(..usize::try_from(read).unwrap_or(CHUNK)) else {
                break;
            };
            if got.is_empty() {
                break;
            }
            body.extend_from_slice(got);
            if body.len() > super::LIMIT {
                return Err(FetchReleasesError::TooLong);
            }
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_running_version_is_the_crate_version() {
        assert_eq!(running().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
