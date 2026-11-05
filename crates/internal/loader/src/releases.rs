//! Whether a newer Enshwick release exists: asked of GitHub once per start on
//! a thread of its own, shown on the Mods screen, logged on every side.

use semver::Version;

/// A release's page, without its version.
const PAGE: &str = "https://github.com/enshwick/enshwick/releases/tag/v";
/// The most bytes of the list read; twenty releases with their notes take
/// well under it.
#[cfg_attr(not(windows), expect(dead_code, reason = "only Windows asks GitHub"))]
const LIMIT: usize = 4 << 20;

/// One release as GitHub lists it; the other fields are ignored.
#[derive(serde::Deserialize)]
struct Listed {
    tag_name: String,
    #[serde(default)]
    draft: bool,
}

/// The newest release in `listing`, GitHub's JSON list of releases, that is
/// newer than `running`; a prerelease counts only while `running` is one,
/// and a tag that is no `v<version>` is skipped.
///
/// # Errors
///
/// When `listing` is no JSON list of releases.
pub fn newer(listing: &[u8], running: &Version) -> Result<Option<Version>, serde_json::Error> {
    let listed: Vec<Listed> = serde_json::from_slice(listing)?;
    Ok(listed
        .into_iter()
        .filter(|release| !release.draft)
        .filter_map(|release| Version::parse(release.tag_name.strip_prefix('v')?).ok())
        .filter(|version| !running.pre.is_empty() || version.pre.is_empty())
        .filter(|version| version > running)
        .max())
}

/// The page of release `version`, built here rather than read from GitHub's
/// answer, so the screen shows no address the network chose.
#[must_use]
pub fn page(version: &Version) -> String {
    format!("{PAGE}{version}")
}

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
            match fetch::list() {
                Ok(listing) => match newer(&listing, &running) {
                    Ok(Some(version)) => {
                        crate::log!(
                            INFO,
                            "Enshwick {version} is available, this is {running}: {}",
                            page(&version)
                        );
                        let _first = loader.newer_release.set(version);
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

    use windows_sys::Win32::Foundation::GetLastError;
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

    /// Why the list of releases did not arrive.
    #[derive(Debug, thiserror::Error)]
    pub(super) enum FetchReleasesError {
        /// A Windows HTTP call failed: no network, no answer in time, or TLS.
        #[error("{call} failed with Windows error {code}")]
        Call { call: &'static str, code: u32 },
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
        // SAFETY: reads the calling thread's last error; no preconditions.
        let code = unsafe { GetLastError() };
        FetchReleasesError::Call { call, code }
    }

    /// `ok`, a Windows HTTP `BOOL`, as the failure of `call` when it is false.
    fn check(ok: i32, call: &'static str) -> Result<(), FetchReleasesError> {
        if ok == 0 { Err(failed(call)) } else { Ok(()) }
    }

    /// GitHub's JSON list of the newest releases, over HTTPS with the system's
    /// proxy settings.
    pub(super) fn list() -> Result<Vec<u8>, FetchReleasesError> {
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
                w!("/repos/enshwick/enshwick/releases?per_page=20"),
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

    fn version(text: &str) -> Version {
        Version::parse(text).unwrap()
    }

    fn listing(tags: &[(&str, bool)]) -> Vec<u8> {
        let releases: Vec<String> = tags
            .iter()
            .map(|(tag, draft)| {
                format!(r#"{{"tag_name":"{tag}","draft":{draft},"prerelease":true,"body":"x"}}"#)
            })
            .collect();
        format!("[{}]", releases.join(",")).into_bytes()
    }

    #[test]
    fn a_newer_alpha_is_found_while_an_alpha_runs() {
        let list = listing(&[("v1.0.0-alpha.13", false), ("v1.0.0-alpha.12", false)]);
        assert_eq!(
            newer(&list, &version("1.0.0-alpha.12")).unwrap(),
            Some(version("1.0.0-alpha.13"))
        );
    }

    #[test]
    fn the_running_release_and_older_ones_are_not_newer() {
        let list = listing(&[("v1.0.0-alpha.12", false), ("v1.0.0-alpha.9", false)]);
        assert_eq!(newer(&list, &version("1.0.0-alpha.12")).unwrap(), None);
    }

    #[test]
    fn the_highest_version_wins_whatever_the_order() {
        let list = listing(&[
            ("v1.0.0-alpha.13", false),
            ("v1.0.0-alpha.20", false),
            ("v1.0.0-alpha.14", false),
        ]);
        assert_eq!(
            newer(&list, &version("1.0.0-alpha.12")).unwrap(),
            Some(version("1.0.0-alpha.20"))
        );
    }

    #[test]
    fn a_release_runs_ahead_of_alphas_but_ignores_them() {
        let list = listing(&[("v1.1.0-alpha.1", false), ("v1.0.1", false)]);
        assert_eq!(
            newer(&list, &version("1.0.0")).unwrap(),
            Some(version("1.0.1"))
        );
        let list = listing(&[("v1.1.0-alpha.1", false)]);
        assert_eq!(newer(&list, &version("1.0.0")).unwrap(), None);
    }

    #[test]
    fn the_release_after_an_alpha_counts_while_the_alpha_runs() {
        let list = listing(&[("v1.0.0", false)]);
        assert_eq!(
            newer(&list, &version("1.0.0-alpha.12")).unwrap(),
            Some(version("1.0.0"))
        );
    }

    #[test]
    fn drafts_and_tags_that_are_no_version_are_skipped() {
        let list = listing(&[
            ("v2.0.0", true),
            ("enshwick-v3.0.0", false),
            ("dev", false),
            ("v1.0.0-alpha.13", false),
        ]);
        assert_eq!(
            newer(&list, &version("1.0.0-alpha.12")).unwrap(),
            Some(version("1.0.0-alpha.13"))
        );
    }

    #[test]
    fn an_answer_that_is_no_list_of_releases_is_an_error() {
        let running = version("1.0.0-alpha.12");
        assert!(newer(br#"{"message":"API rate limit exceeded"}"#, &running).is_err());
        assert!(newer(b"<html>", &running).is_err());
        assert_eq!(newer(b"[]", &running).unwrap(), None);
    }

    #[test]
    fn the_page_is_built_from_the_version() {
        assert_eq!(
            page(&version("1.0.0-alpha.13")),
            "https://github.com/enshwick/enshwick/releases/tag/v1.0.0-alpha.13"
        );
    }

    #[test]
    fn the_running_version_is_the_crate_version() {
        assert_eq!(running().to_string(), env!("CARGO_PKG_VERSION"));
    }
}
