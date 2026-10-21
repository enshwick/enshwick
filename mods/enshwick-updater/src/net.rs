//! HTTPS requests: GitHub's release lists, the documents of `url(...)`
//! sources and release files, each within a time and size limit and without
//! a token.
//!
//! A request that fails on the way (no connection, a server error, a reply
//! cut short) is tried again twice, a little later each time; a refusal is
//! not. GitHub's lists are kept on disk with their `ETag`, and asked again
//! with `If-None-Match`: an unchanged list costs no request of the hourly
//! limit.

use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long one request may take, connection to last byte.
const TIMEOUT: Duration = Duration::from_secs(60);
/// The most bytes of a release list, one release or a source's document.
const MAX_JSON: u64 = 4 << 20;
/// The most bytes of a release archive.
const MAX_ARCHIVE: u64 = 64 << 20;
/// The most bytes of a signature.
const MAX_SIGNATURE: u64 = 4 << 10;
/// How long to wait before each retry of a request that failed on the way.
const RETRIES: [Duration; 2] = [Duration::from_secs(2), Duration::from_secs(8)];
/// The most pages of one release list: 1000 releases.
pub(crate) const MAX_PAGES: usize = 10;
/// Where the next page of a GitHub list may come from.
const API: &str = "https://api.github.com/";
/// How long GitHub is left alone after a refusal that names no time.
const DEFAULT_PAUSE: u64 = 3600;

/// Why a request failed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// GitHub's limit for requests without a token is used up; nothing more
    /// is asked before `until` (Unix seconds).
    RateLimited {
        /// When GitHub takes requests again.
        until: u64,
    },
    /// No such release or file.
    NotFound,
    /// Anything else, with a message for the log.
    Other(String),
}

impl core::fmt::Display for Failure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::RateLimited { until } => write!(
                f,
                "GitHub's request limit is used up for {} more minutes",
                minutes_until(*until, crate::now())
            ),
            Self::NotFound => f.write_str("not found"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

/// Whole minutes from `now` to `until`, at least one.
pub(crate) fn minutes_until(until: u64, now: u64) -> u64 {
    until.saturating_sub(now).div_ceil(60).max(1)
}

/// One reply: its body and, for a paged list, the address of the next page.
#[derive(Debug, Default)]
struct Reply {
    body: Vec<u8>,
    next: Option<String>,
    etag: Option<String>,
}

/// An HTTPS client for releases.
#[derive(Debug)]
pub(crate) struct Client {
    agent: ureq::Agent,
    /// Where GitHub's lists are kept with their `ETag`; none keeps nothing.
    cache: Option<PathBuf>,
}

impl Client {
    /// A client with the updater's limits that keeps GitHub's lists in
    /// `cache`.
    #[must_use]
    pub(crate) fn new(cache: Option<&Path>) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .https_only(true)
            .user_agent(concat!("enshwick-updater/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self {
            agent,
            cache: cache.map(Path::to_path_buf),
        }
    }

    /// The JSON at a GitHub API address.
    ///
    /// # Errors
    ///
    /// When the request fails, GitHub refuses it or the reply is too long.
    pub(crate) fn api(&self, url: &str) -> Result<Vec<u8>, Failure> {
        self.api_reply(url).map(|reply| reply.body)
    }

    /// Every page of a GitHub list from `first` on, following its `next`
    /// links up to [`MAX_PAGES`], until `enough` says the pages so far hold
    /// what the caller needs.
    ///
    /// # Errors
    ///
    /// As [`Client::api`], for any page.
    pub(crate) fn api_pages(
        &self,
        first: &str,
        mut enough: impl FnMut(&[u8]) -> bool,
    ) -> Result<Vec<Vec<u8>>, Failure> {
        let mut pages = Vec::new();
        let mut url = first.to_owned();
        for _ in 0..MAX_PAGES {
            let reply = self.api_reply(&url)?;
            let done = enough(&reply.body);
            pages.push(reply.body);
            match reply.next.filter(|next| next.starts_with(API)) {
                Some(next) if !done => url = next,
                _ => break,
            }
        }
        Ok(pages)
    }

    fn api_reply(&self, url: &str) -> Result<Reply, Failure> {
        let cached = self.cache.as_deref().and_then(|dir| Cached::read(dir, url));
        let etag = cached.as_ref().map(|c| c.etag.as_str());
        match self.get(url, Some("application/vnd.github+json"), etag, MAX_JSON) {
            Ok(None) => cached
                .map(Cached::into_reply)
                .ok_or_else(|| Failure::Other(format!("{url}: unchanged, but nothing is kept"))),
            Ok(Some(reply)) => {
                if let (Some(dir), Some(etag)) = (self.cache.as_deref(), reply.etag.as_deref()) {
                    Cached::write(dir, url, etag, &reply);
                }
                Ok(reply)
            }
            Err(failure) => Err(failure),
        }
    }

    /// The document of a `url(...)` source.
    ///
    /// # Errors
    ///
    /// As [`Client::api`]; a host's request limit counts as any other failure.
    pub(crate) fn document(&self, url: &str) -> Result<Vec<u8>, Failure> {
        self.plain(url, MAX_JSON).map_err(|failure| match failure {
            Failure::RateLimited { .. } => {
                Failure::Other(format!("{url}: too many requests for now"))
            }
            other => other,
        })
    }

    /// A release archive.
    ///
    /// # Errors
    ///
    /// As [`Client::api`].
    pub(crate) fn archive(&self, url: &str) -> Result<Vec<u8>, Failure> {
        self.plain(url, MAX_ARCHIVE)
    }

    /// A build table.
    ///
    /// # Errors
    ///
    /// As [`Client::api`].
    pub(crate) fn table(&self, url: &str) -> Result<Vec<u8>, Failure> {
        self.plain(url, MAX_JSON)
    }

    /// A signature file, as text.
    ///
    /// # Errors
    ///
    /// As [`Client::api`], and when the file is not UTF-8.
    pub(crate) fn signature(&self, url: &str) -> Result<String, Failure> {
        let bytes = self.plain(url, MAX_SIGNATURE)?;
        String::from_utf8(bytes).map_err(|_| Failure::Other("the signature is not text".into()))
    }

    fn plain(&self, url: &str, limit: u64) -> Result<Vec<u8>, Failure> {
        match self.get(url, None, None, limit)? {
            Some(reply) => Ok(reply.body),
            None => Err(Failure::Other(format!("{url}: unexpected 304"))),
        }
    }

    /// `url`, tried again after a failure on the way; `None` when the server
    /// says the copy with `etag` is current.
    fn get(
        &self,
        url: &str,
        accept: Option<&str>,
        etag: Option<&str>,
        limit: u64,
    ) -> Result<Option<Reply>, Failure> {
        let mut waits = RETRIES.iter();
        loop {
            match self.once(url, accept, etag, limit) {
                Err(Attempt::Transient(message)) => match waits.next() {
                    Some(wait) => {
                        enshwick_loader_api::info!("{message}; trying again");
                        std::thread::sleep(*wait);
                    }
                    None => return Err(Failure::Other(message)),
                },
                Err(Attempt::Final(failure)) => return Err(failure),
                Ok(reply) => return Ok(reply),
            }
        }
    }

    fn once(
        &self,
        url: &str,
        accept: Option<&str>,
        etag: Option<&str>,
        limit: u64,
    ) -> Result<Option<Reply>, Attempt> {
        let mut request = self.agent.get(url);
        if let Some(accept) = accept {
            request = request
                .header("Accept", accept)
                .header("X-GitHub-Api-Version", "2022-11-28");
        }
        if let Some(etag) = etag {
            request = request.header("If-None-Match", etag);
        }
        let mut response = request
            .call()
            .map_err(|e| Attempt::Transient(format!("{url}: {e}")))?;
        let status = response.status().as_u16();
        let header = |name: &str| {
            response
                .headers()
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned)
        };
        match status {
            200..=299 => {}
            304 => return Ok(None),
            404 => return Err(Attempt::Final(Failure::NotFound)),
            403 | 429 => {
                let limited = header("x-ratelimit-remaining").as_deref() == Some("0")
                    || header("retry-after").is_some();
                return Err(Attempt::Final(if limited {
                    Failure::RateLimited {
                        until: pause_until(
                            header("retry-after").as_deref(),
                            header("x-ratelimit-reset").as_deref(),
                            crate::now(),
                        ),
                    }
                } else {
                    Failure::Other(format!("{url}: HTTP {status}"))
                }));
            }
            500..=599 => return Err(Attempt::Transient(format!("{url}: HTTP {status}"))),
            _ => {
                return Err(Attempt::Final(Failure::Other(format!(
                    "{url}: HTTP {status}"
                ))));
            }
        }
        let length = header("content-length").and_then(|l| l.parse::<u64>().ok());
        if length.is_some_and(|length| length > limit) {
            return Err(Attempt::Final(Failure::Other(format!(
                "{url}: {} bytes, more than {limit}",
                length.unwrap_or_default()
            ))));
        }
        let next = header("link")
            .as_deref()
            .and_then(next_link)
            .map(str::to_owned);
        let etag = header("etag");
        let body = response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|e| Attempt::Transient(format!("{url}: {e}")))?;
        if let Some(length) = length
            && body.len() as u64 != length
        {
            return Err(Attempt::Transient(format!(
                "{url}: cut short at {} of {length} bytes",
                body.len()
            )));
        }
        Ok(Some(Reply { body, next, etag }))
    }
}

/// How one try of a request ended, when it failed.
enum Attempt {
    /// On the way: worth another try.
    Transient(String),
    /// A refusal or a reply that will not change.
    Final(Failure),
}

/// When GitHub takes requests again after a refusal: `retry-after` seconds
/// from `now`, else its `x-ratelimit-reset` time, else an hour on.
pub(crate) fn pause_until(retry_after: Option<&str>, reset: Option<&str>, now: u64) -> u64 {
    retry_after
        .and_then(|seconds| seconds.trim().parse::<u64>().ok())
        .map(|seconds| now.saturating_add(seconds))
        .or_else(|| {
            reset
                .and_then(|at| at.trim().parse::<u64>().ok())
                .filter(|&at| at > now)
        })
        .unwrap_or(now + DEFAULT_PAUSE)
}

/// The address a `Link` header names as `rel="next"`.
pub(crate) fn next_link(header: &str) -> Option<&str> {
    header.split(',').find_map(|part| {
        let (url, params) = part.trim().split_once(';')?;
        let next = params
            .split(';')
            .any(|param| matches!(param.trim(), "rel=\"next\"" | "rel=next"));
        next.then(|| url.trim().strip_prefix('<')?.strip_suffix('>'))
            .flatten()
    })
}

/// A GitHub list kept on disk: its `ETag`, its `next` link and its body.
#[derive(Debug, serde::Deserialize, serde::Serialize)]
struct Cached {
    url: String,
    etag: String,
    next: Option<String>,
    body: String,
}

impl Cached {
    fn path(dir: &Path, url: &str) -> PathBuf {
        // FNV-1a over the address names its file.
        let hash = url.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
        dir.join(format!("{hash:016x}.json"))
    }

    fn read(dir: &Path, url: &str) -> Option<Self> {
        let text = std::fs::read(Self::path(dir, url)).ok()?;
        serde_json::from_slice::<Self>(&text)
            .ok()
            .filter(|cached| cached.url == url)
    }

    fn write(dir: &Path, url: &str, etag: &str, reply: &Reply) {
        let Ok(body) = std::str::from_utf8(&reply.body) else {
            return;
        };
        let cached = Self {
            url: url.to_owned(),
            etag: etag.to_owned(),
            next: reply.next.clone(),
            body: body.to_owned(),
        };
        let path = Self::path(dir, url);
        let temporary = path.with_extension("tmp");
        let written = std::fs::create_dir_all(dir)
            .and_then(|()| serde_json::to_vec(&cached).map_err(std::io::Error::other))
            .and_then(|bytes| std::fs::write(&temporary, bytes))
            .and_then(|()| std::fs::rename(&temporary, &path));
        if let Err(e) = written {
            enshwick_loader_api::info!("cannot keep {url} in {}: {e}", dir.display());
        }
    }

    fn into_reply(self) -> Reply {
        Reply {
            body: self.body.into_bytes(),
            next: self.next,
            etag: Some(self.etag),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_next_page_comes_from_the_link_header() {
        let header = "<https://api.github.com/repositories/1/releases?per_page=100&page=2>; rel=\"next\", <https://api.github.com/repositories/1/releases?per_page=100&page=5>; rel=\"last\"";
        assert_eq!(
            next_link(header),
            Some("https://api.github.com/repositories/1/releases?per_page=100&page=2")
        );
        let last = "<https://api.github.com/x?page=1>; rel=\"prev\", <https://api.github.com/x?page=1>; rel=\"first\"";
        assert_eq!(next_link(last), None);
        assert_eq!(next_link(""), None);
        assert_eq!(next_link("garbage; rel=\"next\""), None);
    }

    #[test]
    fn a_refusal_names_when_to_ask_again() {
        assert_eq!(pause_until(Some("120"), Some("5000"), 1000), 1120);
        assert_eq!(pause_until(None, Some("5000"), 1000), 5000);
        assert_eq!(pause_until(None, Some("500"), 1000), 1000 + DEFAULT_PAUSE);
        assert_eq!(pause_until(Some("soon"), None, 1000), 1000 + DEFAULT_PAUSE);
        assert_eq!(minutes_until(1061, 1000), 2);
        assert_eq!(minutes_until(900, 1000), 1);
    }

    #[test]
    fn a_kept_list_is_read_back_for_its_own_address_only() {
        let dir =
            std::env::temp_dir().join(format!("enshwick-updater-cache-{}", std::process::id()));
        let reply = Reply {
            body: b"[]".to_vec(),
            next: Some("https://api.github.com/next".into()),
            etag: Some("\"abc\"".into()),
        };
        Cached::write(&dir, "https://api.github.com/a", "\"abc\"", &reply);
        let back = Cached::read(&dir, "https://api.github.com/a").unwrap();
        assert_eq!(back.etag, "\"abc\"");
        let reply = back.into_reply();
        assert_eq!(reply.body, b"[]");
        assert_eq!(reply.next.as_deref(), Some("https://api.github.com/next"));
        assert!(Cached::read(&dir, "https://api.github.com/b").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
