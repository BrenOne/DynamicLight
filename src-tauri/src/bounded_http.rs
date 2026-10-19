//! A single-request HTTP parser. Callers supply transport deadlines and authentication.
use std::{
    collections::BTreeMap,
    io::{Read, Write},
    time::{Duration, Instant},
};
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: BTreeMap<String, String>,
    pub body: Vec<u8>,
}
pub fn read(stream: &mut impl Read, host: &str) -> Result<Request, String> {
    let started = Instant::now();
    let mut bytes = vec![];
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() >= 8192 || started.elapsed() > Duration::from_secs(2) {
            return Err("Header limit exceeded".into());
        }
        let mut byte = [0];
        stream
            .read_exact(&mut byte)
            .map_err(|_| "Incomplete HTTP header")?;
        bytes.push(byte[0]);
    }
    let text = std::str::from_utf8(&bytes).map_err(|_| "Invalid header")?;
    let mut lines = text.split("\r\n");
    let parts: Vec<_> = lines.next().unwrap_or_default().split(' ').collect();
    if parts.len() != 3 || parts[2] != "HTTP/1.1" || !["GET", "POST"].contains(&parts[0]) {
        return Err("Unsupported request".into());
    }
    let mut headers = BTreeMap::new();
    for line in lines.filter(|l| !l.is_empty()) {
        let (key, value) = line.split_once(':').ok_or("Invalid header")?;
        if key.is_empty()
            || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || value.bytes().any(|b| b < 32 && b != 9)
            || headers
                .insert(key.to_ascii_lowercase(), value.trim().to_string())
                .is_some()
        {
            return Err("Invalid or duplicate header".into());
        }
    }
    if headers.get("host").map(String::as_str) != Some(host)
        || headers.contains_key("transfer-encoding")
    {
        return Err("Invalid host or transfer encoding".into());
    }
    let length = match headers.get("content-length") {
        Some(v) => v.parse::<usize>().map_err(|_| "Invalid length")?,
        None if parts[0] == "GET" => 0,
        None => return Err("Content-Length required".into()),
    };
    if length > 32768 {
        return Err("Body too large".into());
    }
    let mut body = vec![0; length];
    let mut offset = 0;
    while offset < length {
        if started.elapsed() > Duration::from_secs(2) {
            return Err("Request timeout".into());
        }
        let n = stream
            .read(&mut body[offset..])
            .map_err(|_| "Incomplete body")?;
        if n == 0 {
            return Err("Incomplete body".into());
        }
        offset += n;
    }
    Ok(Request {
        method: parts[0].into(),
        path: parts[1].into(),
        headers,
        body,
    })
}
pub fn equal(actual: &str, expected: &str) -> bool {
    actual.len() == expected.len()
        && actual
            .bytes()
            .zip(expected.bytes())
            .fold(0u8, |d, (a, b)| d | (a ^ b))
            == 0
}
pub fn reply(stream: &mut impl Write, code: u16, content_type: &str, body: &[u8], extra: &str) {
    let header = format!("HTTP/1.1 {code} Response\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'self'; frame-ancestors 'none'; base-uri 'none'; form-action 'self'\r\n{extra}\r\n",body.len());
    let _ = stream
        .write_all(header.as_bytes())
        .and_then(|_| stream.write_all(body))
        .and_then(|_| stream.flush());
}
pub struct Rate {
    tokens: f32,
    last: Instant,
}
impl Rate {
    pub fn new() -> Self {
        Self {
            tokens: 20.0,
            last: Instant::now(),
        }
    }
    pub fn accept(&mut self) -> bool {
        self.tokens = (self.tokens + self.last.elapsed().as_secs_f32() * 10.0).min(20.0);
        self.last = Instant::now();
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_ambiguous_hosts_lengths_and_headers() {
        for headers in [
            "Host: evil\r\n",
            "Host: local\r\nHost: local\r\n",
            "Host: local\r\nTransfer-Encoding: chunked\r\n",
            "Host: local\r\nContent-Length: 40000\r\n",
        ] {
            assert!(read(
                &mut format!("POST / HTTP/1.1\r\n{headers}\r\n").as_bytes(),
                "local"
            )
            .is_err());
        }
        assert!(read(
            &mut b"POST / HTTP/1.1\r\nHost: local\r\nContent-Length: 2\r\n\r\n{}".as_slice(),
            "local"
        )
        .is_ok());
        assert!(!equal("abcd", "abce"));
    }
}
