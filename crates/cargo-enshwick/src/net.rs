//! HTTPS downloads through the system's `curl`, which Windows 10 and later,
//! macOS and Linux distributions carry: it uses the system's certificates
//! and proxy settings, and the tool builds without a TLS stack of its own.

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
    let mut curl = Command::new("curl");
    curl.args([
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
    let out = curl
        .arg(url)
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
