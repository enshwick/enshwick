//! HTTPS requests: GitHub's release lists, the documents of `url(...)`
//! sources and release files, each within a time and size limit and without
//! a token.

use std::time::Duration;

/// How long one request may take, connection to last byte.
const TIMEOUT: Duration = Duration::from_secs(60);
/// The most bytes of a release list, one release or a source's document.
const MAX_JSON: u64 = 4 << 20;
/// The most bytes of a release archive.
const MAX_ARCHIVE: u64 = 64 << 20;
/// The most bytes of a signature.
const MAX_SIGNATURE: u64 = 4 << 10;

/// Why a request failed.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Failure {
    /// GitHub's limit for requests without a token is used up; nothing more
    /// is asked until the next check.
    RateLimited,
    /// No such release or file.
    NotFound,
    /// Anything else, with a message for the log.
    Other(String),
}

impl core::fmt::Display for Failure {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::RateLimited => f.write_str("GitHub's request limit is used up for now"),
            Self::NotFound => f.write_str("not found"),
            Self::Other(message) => f.write_str(message),
        }
    }
}

/// An HTTPS client for releases.
#[derive(Debug)]
pub(crate) struct Client {
    agent: ureq::Agent,
}

impl Client {
    /// A client with the updater's limits.
    #[must_use]
    pub(crate) fn new() -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(TIMEOUT))
            .http_status_as_error(false)
            .https_only(true)
            .user_agent(concat!("enshwick-updater/", env!("CARGO_PKG_VERSION")))
            .build()
            .into();
        Self { agent }
    }

    /// The JSON at a GitHub API address.
    ///
    /// # Errors
    ///
    /// When the request fails, GitHub refuses it or the reply is too long.
    pub(crate) fn api(&self, url: &str) -> Result<Vec<u8>, Failure> {
        self.get(url, Some("application/vnd.github+json"), MAX_JSON)
    }

    /// The document of a `url(...)` source.
    ///
    /// # Errors
    ///
    /// As [`Client::api`]; a host's request limit counts as any other failure.
    pub(crate) fn document(&self, url: &str) -> Result<Vec<u8>, Failure> {
        self.get(url, None, MAX_JSON)
            .map_err(|failure| match failure {
                Failure::RateLimited => Failure::Other(format!("{url}: too many requests for now")),
                other => other,
            })
    }

    /// A release archive.
    ///
    /// # Errors
    ///
    /// As [`Client::api`].
    pub(crate) fn archive(&self, url: &str) -> Result<Vec<u8>, Failure> {
        self.get(url, None, MAX_ARCHIVE)
    }

    /// A signature file, as text.
    ///
    /// # Errors
    ///
    /// As [`Client::api`], and when the file is not UTF-8.
    pub(crate) fn signature(&self, url: &str) -> Result<String, Failure> {
        let bytes = self.get(url, None, MAX_SIGNATURE)?;
        String::from_utf8(bytes).map_err(|_| Failure::Other("the signature is not text".into()))
    }

    fn get(&self, url: &str, accept: Option<&str>, limit: u64) -> Result<Vec<u8>, Failure> {
        let mut request = self.agent.get(url);
        if let Some(accept) = accept {
            request = request
                .header("Accept", accept)
                .header("X-GitHub-Api-Version", "2022-11-28");
        }
        let mut response = request
            .call()
            .map_err(|e| Failure::Other(format!("{url}: {e}")))?;
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
            404 => return Err(Failure::NotFound),
            403 | 429
                if header("x-ratelimit-remaining").as_deref() == Some("0")
                    || header("retry-after").is_some() =>
            {
                return Err(Failure::RateLimited);
            }
            _ => return Err(Failure::Other(format!("{url}: HTTP {status}"))),
        }
        response
            .body_mut()
            .with_config()
            .limit(limit)
            .read_to_vec()
            .map_err(|e| Failure::Other(format!("{url}: {e}")))
    }
}

impl Default for Client {
    fn default() -> Self {
        Self::new()
    }
}
