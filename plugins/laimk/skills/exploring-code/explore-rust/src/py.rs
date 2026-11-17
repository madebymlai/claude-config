//! The Python semantics explore's output depends on: what counts as whitespace, how text that is not UTF-8 decodes,
//! and how paths normalise, split and print. Python's explore defines the output, so these follow CPython rather
//! than Rust's own string and path functions wherever the two differ.

use std::borrow::Cow;

/// `str.isspace`: Rust's White_Space property plus the four information separators U+001C..U+001F.
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\x1c'..='\x1f').contains(&c)
}

pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

pub fn lstrip(s: &str) -> &str {
    s.trim_start_matches(is_space)
}

pub fn rstrip(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// The leading whitespace of S.
pub fn indent(s: &str) -> &str {
    &s[..s.len() - lstrip(s).len()]
}

pub fn char_len(s: &str) -> usize {
    s.chars().count()
}

/// `s[start:end]` by characters, with Python's clamping.
pub fn slice(s: &str, start: usize, end: usize) -> &str {
    let byte = |n: usize| s.char_indices().nth(n).map_or(s.len(), |(i, _)| i);
    let (a, b) = (byte(start), byte(end));
    if a >= b { "" } else { &s[a..b] }
}

/// DATA as UTF-8, or as Latin-1 when it is not UTF-8, which keeps every byte as one character.
pub fn decode(data: &[u8]) -> Cow<'_, str> {
    match std::str::from_utf8(data) {
        Ok(s) => Cow::Borrowed(s),
        Err(_) => encoding_rs::mem::decode_latin1(data),
    }
}

/// A file name as Python prints it: `os.fsdecode`, whose surrogate escapes print as `\udcXX` under
/// `errors="backslashreplace"`.
pub fn display(name: &[u8]) -> String {
    let mut out = String::with_capacity(name.len());
    for chunk in name.utf8_chunks() {
        out.push_str(chunk.valid());
        for b in chunk.invalid() {
            out.push_str(&format!("\\udc{b:02x}"));
        }
    }
    out
}

/// The order Python sorts file names in: by code point, with each undecodable byte at its surrogate escape
/// U+DC80..U+DCFF. UTF-8 bytes order as their code points do, so each escape is written as the three bytes that
/// would encode it.
pub fn sort_key(name: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(name.len());
    for chunk in name.utf8_chunks() {
        out.extend_from_slice(chunk.valid().as_bytes());
        for &b in chunk.invalid() {
            let cp = 0xDC00u32 + b as u32;
            out.extend_from_slice(&[0xE0 | (cp >> 12) as u8, 0x80 | ((cp >> 6) & 0x3F) as u8, 0x80 | (cp & 0x3F) as u8]);
        }
    }
    out
}

/// `os.path.normpath`.
pub fn normpath(path: &[u8]) -> Vec<u8> {
    if path.is_empty() {
        return b".".to_vec();
    }
    let slashes = if path.starts_with(b"//") && !path.starts_with(b"///") {
        2
    } else if path.starts_with(b"/") {
        1
    } else {
        0
    };
    let mut comps: Vec<&[u8]> = Vec::new();
    for comp in path.split(|&b| b == b'/') {
        if comp.is_empty() || comp == b"." {
            continue;
        }
        if comp != b".." || (slashes == 0 && comps.is_empty()) || comps.last() == Some(&&b".."[..]) {
            comps.push(comp);
        } else if !comps.is_empty() {
            comps.pop();
        }
    }
    let mut out = vec![b'/'; slashes];
    out.extend_from_slice(&comps.join(&b'/'));
    if out.is_empty() { b".".to_vec() } else { out }
}

/// `os.path.abspath`, against the working directory CWD.
pub fn abspath(path: &[u8], cwd: &[u8]) -> Vec<u8> {
    if path.starts_with(b"/") {
        normpath(path)
    } else {
        normpath(&[cwd, b"/", path].concat())
    }
}

/// `os.path.relpath(path)` from the working directory CWD.
pub fn relpath(path: &[u8], cwd: &[u8]) -> Vec<u8> {
    let parts = |p: Vec<u8>| -> Vec<Vec<u8>> { p.split(|&b| b == b'/').filter(|c| !c.is_empty()).map(<[u8]>::to_vec).collect() };
    let (start, target) = (parts(abspath(cwd, cwd)), parts(abspath(path, cwd)));
    let common = start.iter().zip(&target).take_while(|(a, b)| a == b).count();
    let mut rel: Vec<Vec<u8>> = vec![b"..".to_vec(); start.len() - common];
    rel.extend(target[common..].iter().cloned());
    if rel.is_empty() { b".".to_vec() } else { rel.join(&b'/') }
}

/// `os.path.dirname`.
pub fn dirname(path: &[u8]) -> &[u8] {
    let head = &path[..path.iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1)];
    if !head.is_empty() && head.iter().any(|&b| b != b'/') {
        let end = head.iter().rposition(|&b| b != b'/').map_or(0, |i| i + 1);
        &head[..end]
    } else {
        head
    }
}

/// The extension `os.path.splitext` finds, without its dot: none for a name whose only dots lead it.
pub fn extension(path: &[u8]) -> &[u8] {
    let sep = path.iter().rposition(|&b| b == b'/').map_or(0, |i| i + 1);
    match path.iter().rposition(|&b| b == b'.') {
        Some(dot) if dot > sep && path[sep..dot].iter().any(|&b| b != b'.') => &path[dot + 1..],
        _ => b"",
    }
}

/// `str.isdigit` over ASCII digits; see the README on the digits of other scripts.
pub fn is_digits(s: &[u8]) -> bool {
    !s.is_empty() && s.iter().all(u8::is_ascii_digit)
}

/// `int(s)` of an `is_digits` string, saturating where Python's integers would not.
pub fn int(s: &[u8]) -> u64 {
    s.iter().fold(0u64, |n, &d| n.saturating_mul(10).saturating_add((d - b'0') as u64))
}

/// `plural(n, word)` from explore.
pub fn plural(n: usize, word: &str) -> String {
    let suffix = if n == 1 { "" } else if word == "match" { "es" } else { "s" };
    format!("{n} {word}{suffix}")
}

#[cfg(test)]
mod tests {
    use super::*;

    // expected values are CPython 3's results for the same calls
    #[test]
    fn normpath_matches_posixpath() {
        for (path, want) in [
            ("", "."), ("./a/", "a"), ("a//b/./c/..", "a/b"), ("../x", "../x"), ("/../a", "/a"), ("//a", "//a"),
            ("///a", "/a"), ("a/../..", ".."), ("./", "."), ("a/../../b", "../b"),
        ] {
            assert_eq!(normpath(path.as_bytes()), want.as_bytes(), "{path:?}");
        }
    }

    #[test]
    fn relpath_matches_posixpath() {
        assert_eq!(relpath(b"/r/a/b", b"/r"), b"a/b");
        assert_eq!(relpath(b"/r", b"/r/a"), b"..");
        assert_eq!(relpath(b"/x/y", b"/r/a"), b"../../x/y");
        assert_eq!(relpath(b"/r", b"/r"), b".");
    }

    #[test]
    fn dirname_matches_posixpath() {
        assert_eq!(dirname(b"a/b"), b"a");
        assert_eq!(dirname(b"a"), b"");
        assert_eq!(dirname(b"/a"), b"/");
        assert_eq!(dirname(b"a//b"), b"a");
    }

    #[test]
    fn extension_matches_splitext() {
        assert_eq!(extension(b"a/b.rs"), b"rs");
        assert_eq!(extension(b".rs"), b"");
        assert_eq!(extension(b"skip/..rs"), b"");
        assert_eq!(extension(b"..x.rs"), b"rs");
        assert_eq!(extension(b"a.d/b"), b"");
        assert_eq!(extension(b"a."), b"");
    }

    #[test]
    fn whitespace_includes_the_separators_python_strips() {
        assert_eq!(strip("\x1c\u{a0} x \u{85}"), "x");
        assert_eq!(rstrip("x\r\n"), "x");
    }

    #[test]
    fn undecodable_names_print_and_sort_as_surrogate_escapes() {
        assert_eq!(display(b"caf\xe9.rs"), "caf\\udce9.rs");
        // U+4E00 sorts before the escape of 0xE4 in Python, though its first UTF-8 byte is 0xE4 too
        assert!(sort_key("\u{4e00}".as_bytes()) < sort_key(b"\xe4."));
    }

    #[test]
    fn latin1_keeps_every_byte_as_its_code_point() {
        assert_eq!(decode(b"\x85\xe4"), "\u{85}\u{e4}");
        assert_eq!(decode("ä".as_bytes()), "ä");
    }
}
