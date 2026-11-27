//! A safe, bounded HTTP/1.x request parser foundation for tsn.
//!
//! This crate parses request bytes without opening sockets or running a server.
//! It is not yet connected to compiled tsn programs. The returned request fields
//! borrow directly from the input buffer, and `consumed` identifies where a
//! subsequent pipelined request begins.

use std::error::Error;
use std::fmt;

/// Resource limits applied while parsing a request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Maximum bytes in the request line, including its CRLF.
    pub request_line_bytes: usize,
    /// Maximum bytes in all header lines, including CRLFs and the final CRLF.
    pub header_bytes: usize,
    /// Maximum number of header fields.
    pub header_count: usize,
    /// Maximum body bytes accepted.
    pub body_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            request_line_bytes: 8 * 1024,
            header_bytes: 32 * 1024,
            header_count: 100,
            body_bytes: 1024 * 1024,
        }
    }
}

/// A parsed HTTP request with all textual and body views borrowed from input.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Request<'a> {
    pub method: &'a str,
    pub target: &'a str,
    pub version: Version,
    pub headers: Vec<Header<'a>>,
    pub body: &'a [u8],
    pub keep_alive: bool,
    /// Number of input bytes belonging to this request.
    pub consumed: usize,
}

/// Supported HTTP protocol versions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Version {
    Http10,
    Http11,
}

/// A borrowed header field. The name preserves its original casing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Header<'a> {
    pub name: &'a str,
    pub value: &'a str,
}

/// Request parsing errors.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParseError {
    Incomplete,
    InvalidRequestLine,
    InvalidMethod,
    InvalidTarget,
    UnsupportedVersion,
    InvalidLineEnding,
    InvalidHeader,
    TooManyHeaders,
    HeaderLimitExceeded,
    RequestLineLimitExceeded,
    BodyLimitExceeded,
    InvalidContentLength,
    ConflictingContentLength,
    TransferEncodingNotSupported,
    InvalidUtf8,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::Incomplete => "incomplete HTTP request",
            Self::InvalidRequestLine => "invalid HTTP request line",
            Self::InvalidMethod => "invalid HTTP method",
            Self::InvalidTarget => "invalid HTTP request target",
            Self::UnsupportedVersion => "unsupported HTTP version",
            Self::InvalidLineEnding => "HTTP lines must end with CRLF",
            Self::InvalidHeader => "invalid HTTP header",
            Self::TooManyHeaders => "too many HTTP headers",
            Self::HeaderLimitExceeded => "HTTP headers exceed configured limit",
            Self::RequestLineLimitExceeded => "request line exceeds configured limit",
            Self::BodyLimitExceeded => "request body exceeds configured limit",
            Self::InvalidContentLength => "invalid Content-Length",
            Self::ConflictingContentLength => "duplicate or conflicting Content-Length",
            Self::TransferEncodingNotSupported => "request Transfer-Encoding is not supported",
            Self::InvalidUtf8 => "HTTP request contains invalid UTF-8",
        };
        f.write_str(message)
    }
}

impl Error for ParseError {}

/// Parse one complete HTTP/1.0 or HTTP/1.1 request from `input`.
///
/// Returns [`ParseError::Incomplete`] when more bytes are needed. No input is
/// consumed or copied; body bytes after Content-Length remain available as the
/// next pipelined request.
pub fn parse_request<'a>(input: &'a [u8], limits: Limits) -> Result<Request<'a>, ParseError> {
    let (request_line, mut cursor) = line(
        input,
        0,
        limits.request_line_bytes,
        ParseError::RequestLineLimitExceeded,
    )?;
    let line_text = std::str::from_utf8(request_line).map_err(|_| ParseError::InvalidUtf8)?;
    let mut parts = line_text.split(' ');
    let method = parts.next().ok_or(ParseError::InvalidRequestLine)?;
    let target = parts.next().ok_or(ParseError::InvalidRequestLine)?;
    let version_text = parts.next().ok_or(ParseError::InvalidRequestLine)?;
    if parts.next().is_some() || method.is_empty() || target.is_empty() {
        return Err(ParseError::InvalidRequestLine);
    }
    if !method.bytes().all(is_token) {
        return Err(ParseError::InvalidMethod);
    }
    if target.bytes().any(|b| b <= 0x20 || b == 0x7f) {
        return Err(ParseError::InvalidTarget);
    }
    let version = match version_text {
        "HTTP/1.0" => Version::Http10,
        "HTTP/1.1" => Version::Http11,
        _ => return Err(ParseError::UnsupportedVersion),
    };

    let headers_start = cursor;
    let mut headers = Vec::new();
    let mut content_length = None;
    let mut connection_close = false;
    let mut connection_keep_alive = false;
    loop {
        let (header_line, next) = line(
            input,
            cursor,
            limits.header_bytes,
            ParseError::HeaderLimitExceeded,
        )?;
        cursor = next;
        if cursor - headers_start > limits.header_bytes {
            return Err(ParseError::HeaderLimitExceeded);
        }
        if header_line.is_empty() {
            break;
        }
        if headers.len() >= limits.header_count {
            return Err(ParseError::TooManyHeaders);
        }
        let colon = header_line
            .iter()
            .position(|&b| b == b':')
            .ok_or(ParseError::InvalidHeader)?;
        let raw_name = &header_line[..colon];
        if raw_name.is_empty() || !raw_name.iter().copied().all(is_token) {
            return Err(ParseError::InvalidHeader);
        }
        let name = std::str::from_utf8(raw_name).map_err(|_| ParseError::InvalidUtf8)?;
        let raw_value = trim_ows(&header_line[colon + 1..]);
        if raw_value
            .iter()
            .any(|&b| (b < 0x20 && b != b'\t') || b == 0x7f)
        {
            return Err(ParseError::InvalidHeader);
        }
        let value = std::str::from_utf8(raw_value).map_err(|_| ParseError::InvalidUtf8)?;
        if name.eq_ignore_ascii_case("content-length") {
            let parsed = parse_decimal(value.as_bytes()).ok_or(ParseError::InvalidContentLength)?;
            if content_length.replace(parsed).is_some() {
                return Err(ParseError::ConflictingContentLength);
            }
        } else if name.eq_ignore_ascii_case("transfer-encoding") {
            return Err(ParseError::TransferEncodingNotSupported);
        } else if name.eq_ignore_ascii_case("connection") {
            for token in value.split(',').map(str::trim) {
                connection_close |= token.eq_ignore_ascii_case("close");
                connection_keep_alive |= token.eq_ignore_ascii_case("keep-alive");
            }
        }
        headers.push(Header { name, value });
    }
    let body_len = content_length.unwrap_or(0);
    if body_len > limits.body_bytes {
        return Err(ParseError::BodyLimitExceeded);
    }
    let body_end = cursor
        .checked_add(body_len)
        .ok_or(ParseError::BodyLimitExceeded)?;
    if input.len() < body_end {
        return Err(ParseError::Incomplete);
    }
    let body = &input[cursor..body_end];
    let keep_alive = match version {
        Version::Http11 => !connection_close,
        Version::Http10 => connection_keep_alive && !connection_close,
    };
    Ok(Request {
        method,
        target,
        version,
        headers,
        body,
        keep_alive,
        consumed: body_end,
    })
}

fn line(
    input: &[u8],
    start: usize,
    limit: usize,
    limit_error: ParseError,
) -> Result<(&[u8], usize), ParseError> {
    let remaining = input.get(start..).ok_or(ParseError::Incomplete)?;
    for (index, byte) in remaining.iter().enumerate() {
        if *byte == b'\n' {
            if index == 0 || remaining[index - 1] != b'\r' {
                return Err(ParseError::InvalidLineEnding);
            }
            if index + 1 > limit {
                return Err(limit_error);
            }
            return Ok((&remaining[..index - 1], start + index + 1));
        }
        if *byte == b'\r' && remaining.get(index + 1).is_some_and(|next| *next != b'\n') {
            return Err(ParseError::InvalidLineEnding);
        }
        if index + 1 > limit {
            return Err(limit_error);
        }
    }
    if remaining.len() > limit {
        Err(limit_error)
    } else {
        Err(ParseError::Incomplete)
    }
}

fn is_token(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

fn trim_ows(mut bytes: &[u8]) -> &[u8] {
    while bytes.first().is_some_and(|b| *b == b' ' || *b == b'\t') {
        bytes = &bytes[1..];
    }
    while bytes.last().is_some_and(|b| *b == b' ' || *b == b'\t') {
        bytes = &bytes[..bytes.len() - 1];
    }
    bytes
}

fn parse_decimal(bytes: &[u8]) -> Option<usize> {
    if bytes.is_empty() || !bytes.iter().all(u8::is_ascii_digit) {
        return None;
    }
    bytes.iter().try_fold(0usize, |value, digit| {
        value
            .checked_mul(10)?
            .checked_add(usize::from(*digit - b'0'))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(bytes: &[u8]) -> Result<Request<'_>, ParseError> {
        parse_request(bytes, Limits::default())
    }

    #[test]
    fn parses_get_and_post() {
        let get = parse(b"GET /hello HTTP/1.1\r\nHost: example\r\n\r\n").unwrap();
        assert_eq!(
            (get.method, get.target, get.version),
            ("GET", "/hello", Version::Http11)
        );
        assert!(get.keep_alive);
        assert_eq!(get.headers[0].name, "Host");
        let post = parse(b"POST / HTTP/1.1\r\nContent-Length: 3\r\n\r\nabc").unwrap();
        assert_eq!(post.body, b"abc");
    }

    #[test]
    fn body_length_leaves_pipelined_bytes() {
        let bytes = b"POST / HTTP/1.1\r\nContent-Length: 3\r\n\r\nabcGET /next HTTP/1.1\r\n\r\n";
        let first = parse(bytes).unwrap();
        assert_eq!(&bytes[first.consumed..], b"GET /next HTTP/1.1\r\n\r\n");
        assert_eq!(parse(&bytes[first.consumed..]).unwrap().target, "/next");
    }

    #[test]
    fn keep_alive_defaults_by_version() {
        assert!(parse(b"GET / HTTP/1.1\r\n\r\n").unwrap().keep_alive);
        assert!(
            !parse(b"GET / HTTP/1.1\r\nConnection: close\r\n\r\n")
                .unwrap()
                .keep_alive
        );
        assert!(!parse(b"GET / HTTP/1.0\r\n\r\n").unwrap().keep_alive);
        assert!(
            parse(b"GET / HTTP/1.0\r\nConnection: keep-alive\r\n\r\n")
                .unwrap()
                .keep_alive
        );
    }

    #[test]
    fn rejects_malformed_lines_headers_and_lengths() {
        for bad in [
            b"GET / HTTP/1.1\n\n".as_slice(),
            b"GET / HTTP/1.1\rX\n\r\n",
            b"GET / HTTP/1.1\r\nBad Header: x\r\n\r\n",
            b"GET / HTTP/1.1\r\nContent-Length: nope\r\n\r\n",
            b"GET / HTTP/1.1\r\nContent-Length: 1\r\nContent-Length: 1\r\n\r\nx",
            b"GET / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
        ] {
            assert!(parse(bad).is_err(), "accepted {bad:?}");
        }
    }

    #[test]
    fn enforces_limits() {
        let limits = Limits {
            body_bytes: 2,
            ..Limits::default()
        };
        assert_eq!(
            parse_request(b"POST / HTTP/1.1\r\nContent-Length: 3\r\n\r\nabc", limits),
            Err(ParseError::BodyLimitExceeded)
        );
        let limits = Limits {
            header_count: 0,
            ..Limits::default()
        };
        assert_eq!(
            parse_request(b"GET / HTTP/1.1\r\nX: y\r\n\r\n", limits),
            Err(ParseError::TooManyHeaders)
        );
    }
}
