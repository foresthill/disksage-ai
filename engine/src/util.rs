// Shared helpers for the DiskSage engine.

use std::path::PathBuf;

/// Human-readable byte size (1024-based, matching the bash `human_size`).
pub fn human(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    let mut n = bytes as f64;
    let mut i = 0;
    while n >= 1024.0 && i < UNITS.len() - 1 {
        n /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", bytes, UNITS[0])
    } else {
        format!("{:.1} {}", n, UNITS[i])
    }
}

/// The user's home directory, cross-platform (HOME on Unix, USERPROFILE on Windows).
pub fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

/// Minimal HTML escaping for text dropped into served pages (attributes included,
/// so single quotes are escaped too).
pub fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// Percent-decode an `application/x-www-form-urlencoded` value (`+` → space,
/// `%XX` → byte). Used to recover paths posted from the delete form.
pub fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let hex = |c: u8| (c as char).to_digit(16);
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < b.len() => match (hex(b[i + 1]), hex(b[i + 2])) {
                (Some(h), Some(l)) => {
                    out.push((h * 16 + l) as u8);
                    i += 3;
                }
                _ => {
                    out.push(b'%');
                    i += 1;
                }
            },
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Percent-encode a string for use in a URL query value: RFC 3986 unreserved
/// characters are kept, everything else (including UTF-8 bytes) becomes `%XX`.
/// Reversed by `percent_decode`. Used to put a finding's path in a `/note?path=`
/// link safely (paths carry `/`, `~`, spaces and non-ASCII).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Minimal JSON string escaping (the engine emits JSON without a serde dependency
/// to stay tiny; this covers the characters that appear in paths and messages).
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_encode_then_decode_roundtrips_a_path() {
        for s in [
            "~/Development/my project/node_modules",
            "~/Documents/島本町 メモ.md",
            "plain",
        ] {
            assert_eq!(percent_decode(&percent_encode(s)), s, "roundtrip: {s}");
        }
    }

    #[test]
    fn percent_encode_keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(percent_encode("aZ9-_.~"), "aZ9-_.~");
        assert_eq!(percent_encode("a/b c"), "a%2Fb%20c");
    }
}
