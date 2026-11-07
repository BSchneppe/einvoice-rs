//! URI references, as RFC 3986 resolves and normalizes them.
//!
//! * [`resolve`] is §5.2.2 "Transform References" for a strict parser, with
//!   §5.2.3 "Merge Paths" and §5.2.4 "Remove Dot Segments".
//! * [`normalize`] is the syntax-based normalization of §6.2.2 (case of the
//!   scheme, the host and percent-encodings, percent-encoded unreserved
//!   characters decoded, dot segments removed) and the scheme-based one of
//!   §6.2.3 (an empty path after an authority is "/", a default port is
//!   dropped), plus the equivalence of `file:/p`, `file:///p` and
//!   `file://localhost/p` (RFC 8089).
//!
//! The components are split as Appendix B splits them:
//! `^(([^:/?#]+):)?(//([^/?#]*))?([^?#]*)(\?([^#]*))?(#(.*))?`.

use std::path::PathBuf;

/// The five components of a URI reference (§3, Appendix B).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Parts<'a> {
    pub scheme: Option<&'a str>,
    pub authority: Option<&'a str>,
    pub path: &'a str,
    pub query: Option<&'a str>,
    pub fragment: Option<&'a str>,
}

/// The scheme of `s` (§3.1: `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`
/// before the first `:`), if it has one. A drive letter (`C:\x`, `C:/x`) is
/// a path on Windows.
pub(crate) fn scheme(s: &str) -> Option<&str> {
    let colon = s.find(':')?;
    let scheme = &s[..colon];
    let mut chars = scheme.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphabetic()
        || !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    {
        return None;
    }
    if cfg!(windows) && scheme.len() == 1 {
        return None;
    }
    Some(scheme)
}

/// Split a URI reference into its components (Appendix B).
pub(crate) fn parse(s: &str) -> Parts<'_> {
    let (scheme, rest) = match scheme(s) {
        Some(scheme) => (Some(scheme), &s[scheme.len() + 1..]),
        None => (None, s),
    };
    let (rest, fragment) = match rest.find('#') {
        Some(i) => (&rest[..i], Some(&rest[i + 1..])),
        None => (rest, None),
    };
    let (rest, query) = match rest.find('?') {
        Some(i) => (&rest[..i], Some(&rest[i + 1..])),
        None => (rest, None),
    };
    let (authority, path) = match rest.strip_prefix("//") {
        Some(after) => {
            let end = after.find('/').unwrap_or(after.len());
            (Some(&after[..end]), &after[end..])
        }
        None => (None, rest),
    };
    Parts {
        scheme,
        authority,
        path,
        query,
        fragment,
    }
}

/// Recompose components (§5.3).
fn recompose(
    scheme: Option<&str>,
    authority: Option<&str>,
    path: &str,
    query: Option<&str>,
    fragment: Option<&str>,
) -> String {
    let mut out = String::with_capacity(path.len() + 32);
    if let Some(scheme) = scheme {
        out.push_str(scheme);
        out.push(':');
    }
    if let Some(authority) = authority {
        out.push_str("//");
        out.push_str(authority);
    }
    out.push_str(path);
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

/// §5.2.4 Remove Dot Segments.
pub(crate) fn remove_dot_segments(path: &str) -> String {
    let mut input = path;
    let mut output = String::with_capacity(path.len());
    while !input.is_empty() {
        if let Some(rest) = input.strip_prefix("../") {
            input = rest; // A
        } else if let Some(rest) = input.strip_prefix("./") {
            input = rest; // A
        } else if input.starts_with("/./") {
            input = &input[2..]; // B: "/./" -> "/"
        } else if input == "/." {
            input = "/"; // B
        } else if input.starts_with("/../") || input == "/.." {
            // C: replace with "/" and drop the last output segment
            input = if input == "/.." { "/" } else { &input[3..] };
            match output.rfind('/') {
                Some(i) => output.truncate(i),
                None => output.clear(),
            }
        } else if input == "." || input == ".." {
            input = ""; // D
        } else {
            // E: the first segment, with its leading "/" if any
            let start = usize::from(input.starts_with('/'));
            let end = input[start..].find('/').map_or(input.len(), |i| i + start);
            output.push_str(&input[..end]);
            input = &input[end..];
        }
    }
    output
}

/// §5.2.3 Merge Paths.
fn merge(base: &Parts<'_>, reference_path: &str) -> String {
    if base.authority.is_some() && base.path.is_empty() {
        return format!("/{reference_path}");
    }
    match base.path.rfind('/') {
        Some(i) => format!("{}{reference_path}", &base.path[..=i]),
        None => reference_path.to_string(),
    }
}

/// §5.2.2 Transform References (strict): the target URI of `reference`
/// against `base`.
pub(crate) fn resolve(base: &str, reference: &str) -> String {
    let r = parse(reference);
    let b = parse(base);
    if r.scheme.is_some() {
        return recompose(
            r.scheme,
            r.authority,
            &remove_dot_segments(r.path),
            r.query,
            r.fragment,
        );
    }
    let (authority, path, query) = if r.authority.is_some() {
        (r.authority, remove_dot_segments(r.path), r.query)
    } else if r.path.is_empty() {
        (b.authority, b.path.to_string(), r.query.or(b.query))
    } else if r.path.starts_with('/') {
        (b.authority, remove_dot_segments(r.path), r.query)
    } else {
        (
            b.authority,
            remove_dot_segments(&merge(&b, r.path)),
            r.query,
        )
    };
    recompose(b.scheme, authority, &path, query, r.fragment)
}

fn is_unreserved(b: u8) -> bool {
    b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~')
}

fn hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// §6.2.2.1 and §6.2.2.2: percent-encodings in upper case, and decoded where
/// they encode an unreserved character.
fn normalize_percent_encoding(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                let decoded = h * 16 + l;
                if is_unreserved(decoded) {
                    out.push(decoded as char);
                } else {
                    out.push('%');
                    out.push(bytes[i + 1].to_ascii_uppercase() as char);
                    out.push(bytes[i + 2].to_ascii_uppercase() as char);
                }
                i += 3;
                continue;
            }
        }
        let ch = s[i..].chars().next().expect("in bounds");
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

/// The default port of `scheme`, for §6.2.3.
fn default_port(scheme: &str) -> Option<&'static str> {
    match scheme {
        "http" | "ws" => Some("80"),
        "https" | "wss" => Some("443"),
        "ftp" => Some("21"),
        _ => None,
    }
}

/// §6.2.2 / §6.2.3: the normal form of an absolute URI (one with a scheme);
/// a reference without a scheme is returned as it is.
pub(crate) fn normalize(uri: &str) -> String {
    let p = parse(uri);
    let Some(scheme) = p.scheme else {
        return uri.to_string();
    };
    let scheme = scheme.to_ascii_lowercase();
    let authority = p.authority.map(|authority| {
        let (userinfo, hostport) = match authority.rfind('@') {
            Some(i) => (Some(&authority[..i]), &authority[i + 1..]),
            None => (None, authority),
        };
        // An IPv6 literal has colons inside its brackets.
        let port_at = match hostport.rfind(']') {
            Some(close) => hostport[close..].find(':').map(|i| i + close),
            None => hostport.rfind(':'),
        };
        let (host, port) = match port_at {
            Some(i) => (&hostport[..i], Some(&hostport[i + 1..])),
            None => (hostport, None),
        };
        let port = port.filter(|port| !port.is_empty() && default_port(&scheme) != Some(*port));
        let mut out = String::new();
        if let Some(userinfo) = userinfo {
            out.push_str(&normalize_percent_encoding(userinfo));
            out.push('@');
        }
        out.push_str(&normalize_percent_encoding(host).to_ascii_lowercase());
        if let Some(port) = port {
            out.push(':');
            out.push_str(port);
        }
        out
    });
    // file:/p, file://localhost/p and file:///p name one local file.
    let authority = if scheme == "file" {
        // A `file:` URI without an authority and with a relative path (not
        // one RFC 8089 allows) is left as it is: its path is not a host.
        match authority.as_deref() {
            None if p.path.starts_with('/') => Some(String::new()),
            Some("localhost") => Some(String::new()),
            _ => authority,
        }
    } else {
        authority
    };
    let mut path = remove_dot_segments(&normalize_percent_encoding(p.path));
    if authority.is_some() && path.is_empty() && default_port(&scheme).is_some() {
        path.push('/');
    }
    recompose(
        Some(&scheme),
        authority.as_deref(),
        &path,
        p.query.map(normalize_percent_encoding).as_deref(),
        p.fragment.map(normalize_percent_encoding).as_deref(),
    )
}

/// The local path a `file:` URI names (no host, or `localhost`; the path
/// percent-decoded), if it names one.
pub(crate) fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let p = parse(uri);
    if !p.scheme?.eq_ignore_ascii_case("file") {
        return None;
    }
    match p.authority {
        None | Some("") => {}
        Some(host) if host.eq_ignore_ascii_case("localhost") => {}
        Some(_) => return None,
    }
    let bytes = p.path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                decoded.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        decoded.push(bytes[i]);
        i += 1;
    }
    let path = String::from_utf8(decoded).ok()?;
    // file:///C:/x is the Windows path C:/x.
    let path = if cfg!(windows) {
        let b = path.as_bytes();
        if b.len() >= 3 && b[0] == b'/' && b[1].is_ascii_alphabetic() && b[2] == b':' {
            path[1..].to_string()
        } else {
            path
        }
    } else {
        path
    };
    Some(PathBuf::from(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_dot_segments_matches_the_rfc_walkthroughs() {
        // §5.2.4, the two examples traced step by step.
        assert_eq!(remove_dot_segments("/a/b/c/./../../g"), "/a/g");
        assert_eq!(remove_dot_segments("mid/content=5/../6"), "mid/6");
        assert_eq!(remove_dot_segments(""), "");
        assert_eq!(remove_dot_segments("/"), "/");
        assert_eq!(remove_dot_segments("../"), "");
        assert_eq!(remove_dot_segments("/a/.."), "/");
    }

    #[test]
    fn parse_splits_like_appendix_b() {
        let p = parse("http://www.ics.uci.edu/pub/ietf/uri/#Related");
        assert_eq!(p.scheme, Some("http"));
        assert_eq!(p.authority, Some("www.ics.uci.edu"));
        assert_eq!(p.path, "/pub/ietf/uri/");
        assert_eq!(p.query, None);
        assert_eq!(p.fragment, Some("Related"));
        let p = parse("a/b?c#d");
        assert_eq!(
            (p.scheme, p.authority, p.path, p.query, p.fragment),
            (None, None, "a/b", Some("c"), Some("d"))
        );
        assert_eq!(scheme("C:/x"), if cfg!(windows) { None } else { Some("C") });
        assert_eq!(scheme("1http://x"), None);
        assert_eq!(scheme("x.xsd"), None);
    }

    #[test]
    fn file_uris_name_paths() {
        assert_eq!(
            file_uri_to_path("file:///a/b%20c.xsd"),
            Some(PathBuf::from("/a/b c.xsd"))
        );
        assert_eq!(
            file_uri_to_path("file://localhost/a"),
            Some(PathBuf::from("/a"))
        );
        assert_eq!(file_uri_to_path("file://server/a"), None);
        assert_eq!(file_uri_to_path("http://h/a"), None);
    }
}
