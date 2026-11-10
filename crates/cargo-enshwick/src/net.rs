//! HTTPS downloads through the system's `curl`, which Windows 10 and later,
//! macOS and Linux distributions carry: it uses the system's certificates
//! and the proxy of the `https_proxy` environment variable, and the tool
//! builds without a TLS stack of its own.

use std::process::Command;

/// How `cargo enshwick` names itself to the servers it asks.
const AGENT: &str = "enshwick (https://github.com/enshwick)";

/// Why a download gave no body.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// curl is not installed or does not start.
    #[error("curl, which downloads from GitHub, did not start; install curl: {0}")]
    Curl(std::io::Error),
    /// The server, the network or a limit refused the download.
    #[error("{url}: {why}")]
    Failed {
        /// What was asked for.
        url: String,
        /// curl's reason.
        why: String,
    },
    /// The body is longer than asked for.
    #[error("{url}: the answer is longer than {limit} bytes")]
    TooLong {
        /// What was asked for.
        url: String,
        /// The byte limit.
        limit: usize,
    },
}

/// The body of a GET of `url` over HTTPS only, redirects included, refused
/// past `limit` bytes or `seconds` seconds; `headers` go with the request.
///
/// # Errors
///
/// [`Error`] when curl does not run, the download fails or passes a limit.
pub fn get(url: &str, headers: &[&str], limit: usize, seconds: u32) -> Result<Vec<u8>, Error> {
    let out = curl(url, headers, limit, seconds)
        .output()
        .map_err(Error::Curl)?;
    if !out.status.success() {
        return Err(Error::Failed {
            url: url.to_owned(),
            why: String::from_utf8_lossy(&out.stderr)
                .trim_start_matches("curl: ")
                .trim()
                .to_owned(),
        });
    }
    if out.stdout.len() > limit {
        return Err(Error::TooLong {
            url: url.to_owned(),
            limit,
        });
    }
    Ok(out.stdout)
}

/// The `curl` command of [`get`].
fn curl(url: &str, headers: &[&str], limit: usize, seconds: u32) -> Command {
    let mut curl = Command::new("curl");
    curl.args([
        // A user's ~/.curlrc could redirect the body or turn TLS checks off.
        "--disable",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--tlsv1.2",
        "--fail",
        "--silent",
        "--show-error",
        "--location",
        "--user-agent",
        AGENT,
    ])
    .args(["--max-time", &seconds.to_string()])
    .args(["--max-filesize", &limit.to_string()]);
    for header in headers {
        curl.args(["--header", header]);
    }
    curl.arg(url);
    curl
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curl_reads_no_curlrc_and_speaks_only_https() {
        let curl = curl("https://example.com/x", &["A: b"], 10, 5);
        let args: Vec<_> = curl.get_args().map(|arg| arg.to_string_lossy()).collect();
        assert_eq!(
            args[0], "--disable",
            "curl reads ~/.curlrc unless this comes first"
        );
        assert!(args.windows(2).any(|pair| pair == ["--proto", "=https"]));
        assert!(
            args.windows(2)
                .any(|pair| pair == ["--proto-redir", "=https"])
        );
        assert!(args.windows(2).any(|pair| pair == ["--max-filesize", "10"]));
        assert_eq!(
            args.last().map(AsRef::as_ref),
            Some("https://example.com/x")
        );
    }
}
