//! Deliberately small HTTP surface: one authenticated POST, bounded reads, no CORS.
use crate::{session::Connection, Command, Request, Response};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const LIMIT: usize = 8192;
const DEADLINE: Duration = Duration::from_secs(2);

struct RateLimit {
    tokens: f64,
    last: Instant,
}
impl RateLimit {
    fn new(now: Instant) -> Self {
        Self {
            tokens: 20.0,
            last: now,
        }
    }
    fn accept(&mut self, now: Instant) -> bool {
        self.tokens = (self.tokens + now.duration_since(self.last).as_secs_f64() * 10.0).min(20.0);
        self.last = now;
        if self.tokens < 1.0 {
            return false;
        }
        self.tokens -= 1.0;
        true
    }
}

// Configure nonblocking mode once before exchanging bytes. In particular, do not
// set socket options between response reads: Darwin rejects setsockopt after a
// peer shutdown even when the response is still buffered and readable.
fn transfer(
    deadline: Instant,
    mut operation: impl FnMut() -> std::io::Result<usize>,
) -> Result<usize, String> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("Request timed out")?;
        match operation() {
            Ok(0) => return Err("Connection closed before the HTTP message was complete".into()),
            Ok(n) => return Ok(n),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                thread::sleep(remaining.min(Duration::from_millis(1)));
            }
            Err(e) => return Err(format!("HTTP connection failed: {e}")),
        }
    }
}
fn write_bytes(stream: &mut TcpStream, bytes: &[u8], deadline: Instant) -> Result<(), String> {
    let mut offset = 0;
    while offset < bytes.len() {
        offset += transfer(deadline, || stream.write(&bytes[offset..]))?;
    }
    Ok(())
}
fn headers(stream: &mut TcpStream, deadline: Instant) -> Result<String, String> {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") {
        let mut b = [0];
        transfer(deadline, || stream.read(&mut b))?;
        bytes.push(b[0]);
        if bytes.len() > LIMIT {
            return Err("HTTP headers too large".into());
        }
    }
    String::from_utf8(bytes).map_err(|_| "Invalid HTTP headers".into())
}
fn read_body(stream: &mut TcpStream, length: usize, deadline: Instant) -> Result<Vec<u8>, String> {
    let mut body = vec![0; length];
    let mut offset = 0;
    while offset < length {
        let n = transfer(deadline, || stream.read(&mut body[offset..]))?;
        offset += n;
    }
    Ok(body)
}
fn fields(head: &str) -> Result<std::collections::HashMap<String, String>, String> {
    let mut fields = std::collections::HashMap::new();
    for line in head.split("\r\n").skip(1).filter(|l| !l.is_empty()) {
        let (key, value) = line.split_once(':').ok_or("Invalid HTTP header")?;
        if key.is_empty()
            || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
            || fields
                .insert(key.to_ascii_lowercase(), value.trim().to_string())
                .is_some()
        {
            return Err("Invalid or duplicate HTTP header".into());
        }
    }
    Ok(fields)
}
fn authenticate(head: &str, connection: &Connection) -> Result<usize, (u16, String)> {
    let reject = |status, text: &str| (status, text.to_string());
    if head.lines().next() != Some("POST /v1/command HTTP/1.1") {
        return Err(reject(404, "Use POST /v1/command"));
    }
    let f = fields(head).map_err(|e| (400, e))?;
    if f.contains_key("origin")
        || f.contains_key("transfer-encoding")
        || f.get("host") != Some(&format!("127.0.0.1:{}", connection.port))
    {
        return Err(reject(403, "Only local non-browser requests are accepted"));
    }
    let expected = format!("Bearer {}", connection.token);
    let actual = f
        .get("authorization")
        .map(String::as_bytes)
        .unwrap_or_default();
    // Fixed length comparison avoids early exit on token contents.
    let different = actual
        .iter()
        .zip(expected.bytes())
        .fold(0u8, |diff, (a, b)| diff | (a ^ b));
    if actual.len() != expected.len() || different != 0 {
        return Err(reject(401, "Invalid automation credential"));
    }
    if f.get("content-type").map(String::as_str) != Some("application/json") {
        return Err(reject(415, "Use application/json"));
    }
    let length = f
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .ok_or_else(|| reject(400, "Content-Length required"))?;
    if length == 0 || length > LIMIT {
        return Err(reject(413, "Request body must be 1–8192 bytes"));
    }
    Ok(length)
}
fn reply(stream: &mut TcpStream, code: u16, response: Response) {
    if let Ok(body) = serde_json::to_vec(&response) {
        let mut bytes = format!("HTTP/1.1 {code} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\nCache-Control: no-store\r\n\r\n", body.len()).into_bytes();
        bytes.extend_from_slice(&body);
        let _ = write_bytes(stream, &bytes, Instant::now() + DEADLINE);
    }
}

/// One connection at a time bounds resources. The absolute read deadline bounds slow clients.
/// A new server has a new token and rate bucket; shutdown revokes queued engine commands too.
pub fn serve(
    listener: TcpListener,
    connection: Connection,
    active: Arc<AtomicBool>,
    dispatch: impl Fn(Command) -> Result<serde_json::Value, String> + Send + 'static,
) -> std::io::Result<()> {
    listener.set_nonblocking(true)?;
    thread::Builder::new()
        .name("local-automation".into())
        .spawn(move || {
            let mut rate = RateLimit::new(Instant::now());
            while active.load(Ordering::Acquire) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(20));
                        continue;
                    }
                    Err(_) => break,
                };
                if stream.set_nonblocking(true).is_err() {
                    continue;
                }
                let allowed = rate.accept(Instant::now());
                let result = (|| {
                    let deadline = Instant::now() + DEADLINE;
                    let head = headers(&mut stream, deadline).map_err(|e| (400, e))?;
                    let authentication = authenticate(&head, &connection);
                    // Consume a bounded, well-framed body even on auth failure so closing
                    // the socket does not reset the connection before the error is read.
                    let length = fields(&head)
                        .ok()
                        .and_then(|f| {
                            f.get("content-length")
                                .and_then(|v| v.parse::<usize>().ok())
                        })
                        .filter(|n| *n <= LIMIT)
                        .unwrap_or(0);
                    let body = read_body(&mut stream, length, deadline).map_err(|e| (400, e))?;
                    if !allowed {
                        return Err((429, "Rate limit: 10 requests/second, burst 20".into()));
                    }
                    authentication?;
                    let request: Request = serde_json::from_slice(&body)
                        .map_err(|_| (400, "Invalid command JSON".into()))?;
                    request.validate().map_err(|e| (400, e))?;
                    if !active.load(Ordering::Acquire) {
                        return Err((401, "Automation session revoked".into()));
                    }
                    dispatch(request.command).map_err(|e| (409, e))
                })();
                let code = result.as_ref().err().map(|e| e.0).unwrap_or(200);
                reply(
                    &mut stream,
                    code,
                    Response::from_result(result.map_err(|e| e.1)),
                );
            }
        })?;
    Ok(())
}

pub fn call(connection: &Connection, command: Command) -> Result<Response, String> {
    connection.validate()?;
    command.validate()?;
    let address = SocketAddrV4::new(Ipv4Addr::LOCALHOST, connection.port);
    let mut stream = TcpStream::connect_timeout(&address.into(), DEADLINE).map_err(|_| {
        "Cannot reach DynamicLight. Start the desktop app and enable Local automation."
    })?;
    stream.set_nonblocking(true).map_err(|e| e.to_string())?;
    let body = serde_json::to_vec(&Request {
        version: crate::VERSION,
        command,
    })
    .map_err(|e| e.to_string())?;
    let mut bytes = format!("POST /v1/command HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", connection.port, connection.token, body.len()).into_bytes();
    bytes.extend_from_slice(&body);
    write_bytes(&mut stream, &bytes, Instant::now() + DEADLINE)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let head = headers(&mut stream, deadline)?;
    let f = fields(&head)?;
    let length = f
        .get("content-length")
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v <= 65536)
        .ok_or("Invalid response length")?;
    let response: Response = serde_json::from_slice(&read_body(&mut stream, length, deadline)?)
        .map_err(|_| "Invalid API response")?;
    if response.version != crate::VERSION {
        return Err("Incompatible API response version".into());
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rate_bucket_bounds_bursts_and_recovers_without_resetting_on_denial() {
        let now = Instant::now();
        let mut rate = RateLimit::new(now);
        for _ in 0..20 {
            assert!(rate.accept(now));
        }
        assert!(!rate.accept(now));
        assert!(!rate.accept(now + Duration::from_millis(50)));
        assert!(rate.accept(now + Duration::from_millis(100)));
        assert!(!rate.accept(now + Duration::from_millis(100)));
        for _ in 0..20 {
            assert!(rate.accept(now + Duration::from_secs(100)));
        }
        assert!(!rate.accept(now + Duration::from_secs(100)));
    }
    #[test]
    fn partial_headers_cannot_hold_a_connection_past_the_deadline() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        server.set_nonblocking(true).unwrap();
        client.write_all(b"POST /").unwrap();
        let start = Instant::now();
        assert!(headers(&mut server, start + Duration::from_millis(100)).is_err());
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn request_body_can_arrive_after_headers() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let c = Connection::new(listener.local_addr().unwrap().port()).unwrap();
        let active = Arc::new(AtomicBool::new(true));
        serve(listener, c.clone(), active.clone(), |_| {
            Ok(serde_json::json!({"accepted":true}))
        })
        .unwrap();
        let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, c.port)).unwrap();
        stream.set_nonblocking(true).unwrap();
        let body = br#"{"version":1,"command":{"command":"status"}}"#;
        let head = head(&c).replace(
            "Content-Length: 42",
            &format!("Content-Length: {}", body.len()),
        );
        let deadline = Instant::now() + DEADLINE;
        write_bytes(&mut stream, head.as_bytes(), deadline).unwrap();
        thread::sleep(Duration::from_millis(60));
        write_bytes(&mut stream, body, deadline).unwrap();
        let response_head = headers(&mut stream, deadline).unwrap();
        let length = fields(&response_head).unwrap()["content-length"]
            .parse()
            .unwrap();
        let response: Response =
            serde_json::from_slice(&read_body(&mut stream, length, deadline).unwrap()).unwrap();
        active.store(false, Ordering::Release);
        assert!(response.ok);
    }
    #[test]
    fn buffered_response_survives_immediate_peer_shutdown() {
        for _ in 0..20 {
            let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
            let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            client.set_nonblocking(true).unwrap();
            let (mut server, _) = listener.accept().unwrap();
            server.set_nonblocking(true).unwrap();
            reply(
                &mut server,
                200,
                Response::from_result(Ok(serde_json::json!({"powered":false}))),
            );
            server.shutdown(std::net::Shutdown::Both).unwrap();
            drop(server);
            let deadline = Instant::now() + DEADLINE;
            let head = headers(&mut client, deadline).unwrap();
            let length = fields(&head).unwrap()["content-length"].parse().unwrap();
            let response: Response =
                serde_json::from_slice(&read_body(&mut client, length, deadline).unwrap()).unwrap();
            assert!(response.ok);
            assert_eq!(response.data.unwrap()["powered"], false);
        }
    }
    fn head(c: &Connection) -> String {
        format!("POST /v1/command HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: 42\r\n\r\n", c.port, c.token)
    }
    #[test]
    fn authentication_and_browser_and_smuggling_boundaries() {
        let c = Connection::new(1234).unwrap();
        let h = head(&c);
        assert_eq!(authenticate(&h, &c).unwrap(), 42);
        for bad in [
            h.replace(&c.token, "wrong"),
            h.replace("127.0.0.1", "localhost"),
            h.replace("POST", "GET"),
            h.replace("Content-Length: 42", "Content-Length: 999999"),
            h.replace(
                "Content-Length: 42",
                "Content-Length: 42\r\nContent-Length: 42",
            ),
            h.replace("Content-Length: 42", "Content-Length: 42\r\nOrigin: null"),
            h.replace(
                "Content-Length: 42",
                "Content-Length: 42\r\nTransfer-Encoding: chunked",
            ),
        ] {
            assert!(authenticate(&bad, &c).is_err());
        }
    }
    #[test]
    fn real_loopback_roundtrip_and_revocation() {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let c = Connection::new(listener.local_addr().unwrap().port()).unwrap();
        let active = Arc::new(AtomicBool::new(true));
        serve(listener, c.clone(), active.clone(), |_| {
            Ok(serde_json::json!({"powered":false}))
        })
        .unwrap();
        assert_eq!(
            call(&c, Command::Status {}).unwrap().data.unwrap()["powered"],
            false
        );
        let mut invalid = c.clone();
        invalid.token = "0".repeat(64);
        assert!(!call(&invalid, Command::Status {}).unwrap().ok);
        active.store(false, Ordering::Release);
        thread::sleep(Duration::from_millis(60));
        if let Ok(response) = call(&c, Command::Status {}) {
            assert!(!response.ok);
        }
    }
}
