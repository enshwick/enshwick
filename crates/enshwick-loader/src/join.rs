//! The client's join check: before it joins a server, it compares the server's
//! mod set with its own and warns when they differ.
//!
//! The game asks to change servers with a request that starts with the server's
//! query `ip:port`. The hook lets the join go ahead and asks the server, over
//! the Steam query protocol, for its `enshwick` rule.

use std::net::{SocketAddr, UdpSocket};
use std::time::Duration;

const QUERY_TIMEOUT: Duration = Duration::from_secs(3);

/// What to tell the player, or `None` when the mod sets match.
#[must_use]
pub fn verdict(ours: &str, ours_mods: &str, server: Option<(&str, &str)>) -> Option<String> {
    match server {
        Some((fingerprint, _)) if fingerprint == ours => None,
        Some((_, mods)) => Some(format!(
            "This server runs a different set of Enshwick mods.\n\nServer: {mods}\nYours: {ours_mods}\n\nInstall the same mods and versions as the server, or things may not work."
        )),
        None if ours_mods.is_empty() => None,
        None => Some(format!(
            "This server runs no Enshwick mods, and yours ({ours_mods}) change the game for both sides. Things may not work as expected."
        )),
    }
}

/// The `ip:port` at the start of a server change request.
#[must_use]
pub fn address(request: &[u8]) -> Option<SocketAddr> {
    let end = request.iter().position(|&b| b == 0)?;
    std::str::from_utf8(&request[..end]).ok()?.parse().ok()
}

/// The value of `key` in an `A2S_RULES` reply.
#[must_use]
pub fn rule(reply: &[u8], key: &str) -> Option<String> {
    let body = reply.strip_prefix(&[0xff, 0xff, 0xff, 0xff, b'E'])?;
    let mut fields = body.get(2..)?.split(|&b| b == 0);
    while let (Some(name), Some(value)) = (fields.next(), fields.next()) {
        if name == key.as_bytes() {
            return String::from_utf8(value.to_vec()).ok();
        }
    }
    None
}

/// Asks `server` for its rules over the Steam query protocol.
///
/// # Errors
///
/// When the server does not answer in time.
pub fn query_rules(server: SocketAddr) -> std::io::Result<Vec<u8>> {
    let socket = UdpSocket::bind(if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })?;
    socket.set_read_timeout(Some(QUERY_TIMEOUT))?;
    socket.connect(server)?;
    let mut request = vec![0xff, 0xff, 0xff, 0xff, b'V', 0xff, 0xff, 0xff, 0xff];
    let mut reply = vec![0u8; 4096];
    for _ in 0..3 {
        socket.send(&request)?;
        let len = socket.recv(&mut reply)?;
        // A challenge comes back first; send it with the next request.
        if len >= 9 && reply[4] == b'A' {
            request.truncate(5);
            request.extend_from_slice(&reply[5..9]);
            continue;
        }
        reply.truncate(len);
        return Ok(reply);
    }
    Err(std::io::Error::other("no rules after three challenges"))
}

#[cfg(windows)]
pub(crate) use runtime::{NEXT, detour};

#[cfg(windows)]
mod runtime {
    use std::sync::OnceLock;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use windows_sys::Win32::UI::WindowsAndMessaging::{
        MB_ICONWARNING, MB_OK, MB_SETFOREGROUND, MB_TOPMOST, MessageBoxW,
    };

    type ServerChange = unsafe extern "system" fn(provider: *mut u8, request: *const u8) -> u64;

    /// The rest of the chain after the loader's link.
    pub(crate) static NEXT: AtomicUsize = AtomicUsize::new(0);
    /// The request holds the address in its first 0x40 bytes.
    const ADDRESS_BYTES: usize = 0x40;

    static OURS: OnceLock<(String, String)> = OnceLock::new();

    /// Sets the client's own fingerprint and mod list.
    pub(crate) fn set_ours(fingerprint: String, mods: String) {
        let _first = OURS.set((fingerprint, mods));
    }

    /// The loader's link on `server_change`.
    pub(crate) unsafe extern "system" fn detour(provider: *mut u8, request: *const u8) -> u64 {
        if !request.is_null() {
            // SAFETY: the request holds at least the address bytes.
            let bytes = unsafe { std::slice::from_raw_parts(request, ADDRESS_BYTES) }.to_vec();
            let spawned = std::thread::Builder::new()
                .name("enshwick-join".into())
                .spawn(move || drop(std::panic::catch_unwind(|| check(&bytes))));
            drop(spawned);
        }
        let next = NEXT.load(Ordering::Acquire);
        // SAFETY: the loader points the slot at the rest of the chain, which has
        // this signature, before the link can run.
        let next: ServerChange = unsafe { core::mem::transmute(next) };
        // SAFETY: forwarded unchanged.
        unsafe { next(provider, request) }
    }

    fn check(request: &[u8]) {
        let (Some(address), Some((ours, ours_mods))) = (super::address(request), OURS.get()) else {
            return;
        };
        let reply = super::query_rules(address).ok();
        let server = reply.as_deref().and_then(|r| {
            Some((
                super::rule(r, super::super::advertise::RULE)?,
                super::rule(r, super::super::advertise::RULE_MODS).unwrap_or_default(),
            ))
        });
        crate::log!(INFO, "joining {address}: server {server:?}, ours {ours}");
        let Some(text) = super::verdict(
            ours,
            ours_mods,
            server.as_ref().map(|(f, m)| (f.as_str(), m.as_str())),
        ) else {
            return;
        };
        let text: Vec<u16> = text.encode_utf16().chain([0]).collect();
        let title: Vec<u16> = "Enshwick".encode_utf16().chain([0]).collect();
        // SAFETY: NUL-terminated strings and no owner window.
        unsafe {
            MessageBoxW(
                core::ptr::null_mut(),
                text.as_ptr(),
                title.as_ptr(),
                MB_OK | MB_ICONWARNING | MB_TOPMOST | MB_SETFOREGROUND,
            )
        };
    }
}

#[cfg(windows)]
pub(crate) use runtime::set_ours;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_sets_say_nothing_and_others_explain() {
        assert_eq!(verdict("ab", "a@1", Some(("ab", "a@1"))), None);
        assert!(
            verdict("ab", "a@1", Some(("cd", "a@2")))
                .unwrap()
                .contains("Server: a@2")
        );
        assert!(
            verdict("ab", "a@1", None)
                .unwrap()
                .contains("no Enshwick mods")
        );
        assert_eq!(verdict("ab", "", None), None);
    }

    #[test]
    fn the_address_is_read_up_to_the_first_nul() {
        let mut request = [0u8; 0x40];
        request[..15].copy_from_slice(b"127.0.0.1:15647");
        assert_eq!(address(&request), Some("127.0.0.1:15647".parse().unwrap()));
    }

    #[test]
    fn a_rule_is_found_in_a_rules_reply() {
        let mut reply = vec![0xff, 0xff, 0xff, 0xff, b'E', 2, 0];
        for part in ["enshwick", "989b6a71", "enshwick.mods", "a@1"] {
            reply.extend_from_slice(part.as_bytes());
            reply.push(0);
        }
        assert_eq!(rule(&reply, "enshwick").as_deref(), Some("989b6a71"));
        assert_eq!(rule(&reply, "enshwick.mods").as_deref(), Some("a@1"));
        assert_eq!(rule(&reply, "missing"), None);
    }
}
