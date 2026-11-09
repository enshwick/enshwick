//! HTTPS downloads through the system's `curl`, which Windows 10 and later,
//! macOS and Linux distributions carry: it uses the system's certificates
//! and the proxy of the `https_proxy` environment variable, and the tool
//! builds without a TLS stack of its own.

use std::process::Command;

use anyhow::{Context as _, bail};

/// How `cargo enshwick` names itself to the servers it asks.
const AGENT: &str = "enshwick (https://github.com/enshwick)";

/// The body of a GET of `url` over HTTPS only, redirects included, refused
/// past `limit` bytes or `seconds` seconds; `headers` go with the request.
pub(crate) fn get(
    url: &str,
    headers: &[&str],
    limit: usize,
    seconds: u32,
) -> anyhow::Result<Vec<u8>> {
    let out = curl(url, headers, limit, seconds)
        .output()
        .context("curl, which downloads from GitHub, did not start; install curl")?;
    if !out.status.success() {
        bail!(
            "{url}: {}",
            String::from_utf8_lossy(&out.stderr)
                .trim_start_matches("curl: ")
                .trim()
        );
    }
    if out.stdout.len() > limit {
        bail!("{url}: the answer is longer than {limit} bytes");
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
