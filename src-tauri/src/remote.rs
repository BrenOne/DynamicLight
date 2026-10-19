//! Explicit, session-only HTTPS LAN remote. No credentials or private keys persist.
use crate::{bounded_http as http, engine::Handle};
use dynamiclight_control::{session::Connection, Command, Request, Response};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant, SystemTime},
};
use tauri::Manager;

// Monotonic time resists clock rollback; wall time includes OS suspend on hosts
// whose monotonic clock stops sleeping. A backwards wall-clock jump fails closed.
struct Age {
    monotonic: Instant,
    wall: SystemTime,
}
impl Age {
    fn new() -> Self {
        Self {
            monotonic: Instant::now(),
            wall: SystemTime::now(),
        }
    }
    fn elapsed(&self) -> Duration {
        self.monotonic
            .elapsed()
            .max(self.wall.elapsed().unwrap_or(Duration::MAX))
    }
}
struct Client {
    id: String,
    name: String,
    hash: Vec<u8>,
    active: Arc<AtomicBool>,
    created: Age,
    seen: Instant,
}
struct Pairing {
    code: String,
    created: Age,
    attempts: u8,
}
#[derive(Default)]
struct Access {
    clients: Vec<Client>,
    pairing: Option<Pairing>,
}
impl Access {
    fn new_code(&mut self) -> Result<(), String> {
        let random = Connection::new(1)?.token;
        let number = u32::from_str_radix(&random[..8], 16)
            .map_err(|_| "Random code unavailable")?
            % 1_000_000;
        self.pairing = Some(Pairing {
            code: format!("{number:06}"),
            created: Age::new(),
            attempts: 0,
        });
        Ok(())
    }
    fn pair(&mut self, code: &str, name: &str) -> Result<String, String> {
        if name.trim().is_empty() || name.chars().count() > 50 || name.chars().any(char::is_control)
        {
            return Err("Use a client name of 1–50 characters".into());
        }
        let pairing = self
            .pairing
            .as_mut()
            .ok_or("Open pairing in the desktop app")?;
        if pairing.created.elapsed() > Duration::from_secs(120) || pairing.attempts >= 5 {
            return Err("Pairing expired; open a new code in the desktop app".into());
        }
        pairing.attempts += 1;
        if !http::equal(code, &pairing.code) {
            return Err("Invalid pairing code".into());
        }
        if self.clients.len() >= 8 {
            return Err("Eight clients already paired; revoke one in the desktop app".into());
        }
        let token = Connection::new(1)?.token;
        let hash = Sha256::digest(token.as_bytes()).to_vec();
        let id = hash[..8].iter().map(|b| format!("{b:02x}")).collect();
        self.clients.push(Client {
            id,
            name: name.trim().into(),
            hash,
            active: Arc::new(AtomicBool::new(true)),
            created: Age::new(),
            seen: Instant::now(),
        });
        self.pairing = None;
        Ok(token)
    }
    fn authorize(&mut self, token: &str) -> Result<Arc<AtomicBool>, String> {
        if token.len() != 64 || !token.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Pair this phone first".into());
        }
        let hash = Sha256::digest(token.as_bytes());
        for client in &mut self.clients {
            let equal = client
                .hash
                .iter()
                .zip(hash.iter())
                .fold(0u8, |d, (a, b)| d | (a ^ b))
                == 0;
            if equal {
                if client.created.elapsed() > Duration::from_secs(86400) {
                    client.active.store(false, Ordering::Release);
                }
                if !client.active.load(Ordering::Acquire) {
                    break;
                }
                client.seen = Instant::now();
                return Ok(client.active.clone());
            }
        }
        Err("Pairing expired or revoked".into())
    }
    fn revoke(&mut self, id: Option<&str>) {
        self.clients.retain(|c| {
            if id.is_none_or(|id| id == c.id) {
                c.active.store(false, Ordering::Release);
                false
            } else {
                true
            }
        });
        if id.is_none() {
            self.pairing = None;
        }
    }
}
struct Session {
    active: Arc<AtomicBool>,
    access: Arc<Mutex<Access>>,
    endpoint: String,
    fingerprint: String,
    certificate: String,
    controller: String,
}
pub struct Remote {
    session: Mutex<Option<Session>>,
}
#[derive(Serialize)]
pub struct Interface {
    address: String,
    name: String,
}
fn private(ip: Ipv4Addr) -> bool {
    ip.is_private() || ip == Ipv4Addr::LOCALHOST
}
fn interfaces() -> Vec<Interface> {
    let mut result = vec![Interface {
        address: "127.0.0.1".into(),
        name: "This computer only (testing)".into(),
    }];
    if let Ok(all) = if_addrs::get_if_addrs() {
        for interface in all {
            if let IpAddr::V4(ip) = interface.ip() {
                if ip.is_private() && !result.iter().any(|i| i.address == ip.to_string()) {
                    result.push(Interface {
                        address: ip.to_string(),
                        name: interface.name,
                    });
                }
            }
        }
    }
    result
}
fn view(state: &Remote) -> Value {
    let session = state.session.lock().unwrap();
    match session.as_ref() {
        None => json!({"enabled":false,"interfaces":interfaces(),"clients":[]}),
        Some(s) => {
            let access = s.access.lock().unwrap();
            let pairing = access
                .pairing
                .as_ref()
                .filter(|p| p.attempts < 5 && p.created.elapsed() < Duration::from_secs(120));
            json!({"enabled":true,"interfaces":interfaces(),"endpoint":s.endpoint,"fingerprint":s.fingerprint,"controller":s.controller,
                "pairingCode":pairing.map(|p|p.code.as_str()),"pairingSeconds":pairing.map(|p|120u64.saturating_sub(p.created.elapsed().as_secs())),
                "clients":access.clients.iter().map(|c|json!({"id":c.id,"name":c.name,"connected":c.seen.elapsed()<Duration::from_secs(15) && c.active.load(Ordering::Acquire),"lastSeenSeconds":c.seen.elapsed().as_secs()})).collect::<Vec<_>>()})
        }
    }
}
pub fn init(app: &tauri::AppHandle) {
    app.manage(Remote {
        session: Mutex::new(None),
    });
}
pub fn stop(app: &tauri::AppHandle) {
    if let Some(remote) = app.try_state::<Remote>() {
        if let Some(s) = remote.session.lock().unwrap().take() {
            s.active.store(false, Ordering::Release);
            s.access.lock().unwrap().revoke(None);
        }
    }
}
// The transport enforces one absolute deadline even inside rustls's handshake loops.
struct Socket {
    stream: TcpStream,
    deadline: Instant,
}
impl Socket {
    fn check(&self) -> std::io::Result<()> {
        if Instant::now() >= self.deadline {
            Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "Remote deadline",
            ))
        } else {
            Ok(())
        }
    }
}
impl Read for Socket {
    fn read(&mut self, b: &mut [u8]) -> std::io::Result<usize> {
        self.check()?;
        self.stream.read(b)
    }
}
impl Write for Socket {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
        self.check()?;
        self.stream.write(b)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.check()?;
        self.stream.flush()
    }
}
fn cookie(headers: &std::collections::BTreeMap<String, String>) -> &str {
    let mut tokens = headers
        .get("cookie")
        .map(String::as_str)
        .unwrap_or_default()
        .split(';')
        .filter_map(|s| s.trim().strip_prefix("__Host-dynamiclight="));
    let token = tokens.next().unwrap_or_default();
    if tokens.next().is_some() {
        ""
    } else {
        token
    }
}
fn permitted(command: &Command) -> bool {
    matches!(
        command,
        Command::Status {}
            | Command::Profiles {}
            | Command::Brightness { .. }
            | Command::Color { .. }
            | Command::Power {
                allow_capture: false,
                ..
            }
            | Command::ApplyProfile {
                allow_capture: false,
                ..
            }
    )
}
fn public_result(command: &Command, value: Value) -> Value {
    if matches!(command, Command::Profiles {}) {
        return value;
    }
    json!({"connected":value["connected"],"requested":value["requested"],"powered":value["powered"],"mode":value["mode"],"brightness":value["brightness"],"paused":value["pauseReason"].as_str().is_some_and(|s|!s.is_empty())})
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PairRequest {
    code: String,
    name: String,
}
type Dispatch = Arc<dyn Fn(Command, Arc<AtomicBool>) -> Result<Value, String> + Send + Sync>;
fn serve(
    listener: TcpListener,
    tls: Arc<rustls::ServerConfig>,
    active: Arc<AtomicBool>,
    access: Arc<Mutex<Access>>,
    host: String,
    dispatch: Dispatch,
) -> Result<(), String> {
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    std::thread::Builder::new().name("paired-remote".into()).spawn(move || {
        let mut rate=http::Rate::new();
        while active.load(Ordering::Acquire) {
            let stream=match listener.accept(){Ok((s,_))=>s,Err(e) if e.kind()==std::io::ErrorKind::WouldBlock=>{std::thread::sleep(Duration::from_millis(20));continue;},Err(_)=>break};
            if !rate.accept(){continue;}
            let _=stream.set_nonblocking(false);
            let _=stream.set_read_timeout(Some(Duration::from_millis(500)));let _=stream.set_write_timeout(Some(Duration::from_millis(500)));
            let Ok(connection)=rustls::ServerConnection::new(tls.clone()) else {continue;};
            let mut stream=rustls::StreamOwned::new(connection,Socket{stream,deadline:Instant::now()+Duration::from_secs(4)});
            let Ok(request)=http::read(&mut stream,&host) else {continue;};
            if !active.load(Ordering::Acquire){continue;}
            if request.method=="GET" {
                let asset=match request.path.as_str(){"/"=>Some(("text/html; charset=utf-8",include_bytes!("../../src/remote/index.html").as_slice())),"/remote.js"=>Some(("text/javascript; charset=utf-8",include_bytes!("../../src/remote/remote.js").as_slice())),"/remote.css"=>Some(("text/css; charset=utf-8",include_bytes!("../../src/remote/remote.css").as_slice())),_=>None};
                if let Some((kind,body))=asset {http::reply(&mut stream,200,kind,body,"");}else{http::reply(&mut stream,404,"text/plain",b"Not found","");} continue;
            }
            if request.headers.get("origin")!=Some(&format!("https://{host}")) || request.headers.get("content-type").map(String::as_str)!=Some("application/json") {
                http::reply(&mut stream,403,"text/plain",b"Same-origin JSON required","");continue;
            }
            if request.path=="/pair" {
                let result=(||->Result<String,String>{
                    let pair:PairRequest=serde_json::from_slice(&request.body).map_err(|_|"Invalid pairing request")?;
                    let mut access=access.lock().unwrap();
                    if !active.load(Ordering::Acquire){return Err("Remote disabled".into());}
                    access.pair(&pair.code,&pair.name)
                })();
                match result {
                    Ok(token)=>http::reply(&mut stream,200,"application/json",b"{\"ok\":true}",&format!("Set-Cookie: __Host-dynamiclight={token}; Path=/; Secure; HttpOnly; SameSite=Strict; Max-Age=86400\r\n")),
                    Err(e)=>http::reply(&mut stream,403,"application/json",json!({"ok":false,"error":e}).to_string().as_bytes(),"")
                } continue;
            }
            if request.path!="/v1/command" {http::reply(&mut stream,404,"text/plain",b"Not found","");continue;}
            let guard=access.lock().unwrap().authorize(cookie(&request.headers));
            let result=match guard {
                Err(e)=>{http::reply(&mut stream,401,"application/json",json!({"ok":false,"error":e}).to_string().as_bytes(),"");continue;},
                Ok(guard)=>(||->Result<Value,String>{
                    let request:Request=serde_json::from_slice(&request.body).map_err(|_|"Invalid command")?;
                    request.validate()?;
                    if !permitted(&request.command){return Err("This remote supports power, brightness, color and saved profiles only; capture must be started on the desktop".into());}
                    if !active.load(Ordering::Acquire){return Err("Remote disabled".into());}
                    let command=request.command;
                    let data=dispatch(command.clone(),guard).map_err(|_|"Command unavailable. Check the desktop app; capture cannot be started remotely.")?;
                    Ok(public_result(&command,data))
                })()
            };
            let code=if result.is_ok(){200}else{400};
            let body=serde_json::to_vec(&Response::from_result(result)).unwrap_or_default();
            if body.len()<=65536 {http::reply(&mut stream,code,"application/json",&body,"");}
        }
        access.lock().unwrap().revoke(None);
    }).map_err(|e|e.to_string())?;
    Ok(())
}
fn certificate(address: &str) -> Result<(rustls::ServerConfig, String, String), String> {
    let rcgen::CertifiedKey { cert, signing_key } =
        rcgen::generate_simple_self_signed(vec![address.into()]).map_err(|e| e.to_string())?;
    let fingerprint = Sha256::digest(cert.der())
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":");
    let key = rustls::pki_types::PrivatePkcs8KeyDer::from(signing_key.serialize_der());
    let mut config = rustls::ServerConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?
    .with_no_client_auth()
    .with_single_cert(vec![cert.der().clone()], key.into())
    .map_err(|e| e.to_string())?;
    config.alpn_protocols = vec![b"http/1.1".to_vec()];
    Ok((config, fingerprint, cert.pem()))
}
#[tauri::command]
pub async fn remote_manage(
    app: tauri::AppHandle,
    window: tauri::WebviewWindow,
    operation: String,
    address: Option<String>,
    controller: Option<String>,
    client: Option<String>,
) -> Result<Value, String> {
    if window.label() != "main" {
        return Err("Manage the remote in the main window".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Remote>();
        match operation.as_str() {
            "status" => {}
            "disable" => stop(&app),
            "enable" => {
                let address = address.ok_or("Choose a local IPv4 interface")?;
                let ip = address
                    .parse::<Ipv4Addr>()
                    .map_err(|_| "Choose a local IPv4 interface")?;
                if !private(ip) || !interfaces().iter().any(|i| i.address == address) {
                    return Err(
                        "Choose an assigned private LAN address or local test interface".into(),
                    );
                }
                let controller = controller.ok_or("Choose a controller")?;
                let target = app
                    .state::<Handle>()
                    .fleet
                    .as_ref()
                    .unwrap()
                    .target(&controller)?;
                let listener = TcpListener::bind((ip, 0)).map_err(|e| e.to_string())?;
                let port = listener.local_addr().map_err(|e| e.to_string())?.port();
                let host = format!("{ip}:{port}");
                let (tls, fingerprint, certificate) = certificate(&address)?;
                let active = Arc::new(AtomicBool::new(true));
                let access = Arc::new(Mutex::new(Access::default()));
                access.lock().unwrap().new_code()?;
                let app_copy = app.clone();
                let dispatch: Dispatch = Arc::new(move |command, guard| {
                    crate::automation::dispatch_to(&app_copy, command, guard, false, &target, true)
                });
                serve(
                    listener,
                    Arc::new(tls),
                    active.clone(),
                    access.clone(),
                    host.clone(),
                    dispatch,
                )?;
                stop(&app);
                *state.session.lock().unwrap() = Some(Session {
                    active,
                    access,
                    endpoint: format!("https://{host}"),
                    fingerprint,
                    certificate,
                    controller,
                });
            }
            "pair" | "revoke" | "revokeAll" => {
                let session = state.session.lock().unwrap();
                let session = session.as_ref().ok_or("Enable the remote first")?;
                let mut access = session.access.lock().unwrap();
                match operation.as_str() {
                    "pair" => access.new_code()?,
                    "revokeAll" => access.revoke(None),
                    _ => access.revoke(Some(client.as_deref().ok_or("Choose a client")?)),
                }
            }
            "certificate" => {
                let session = state.session.lock().unwrap();
                let session = session.as_ref().ok_or("Enable the remote first")?;
                return Ok(json!({"pem":session.certificate,"fingerprint":session.fingerprint}));
            }
            _ => return Err("Unknown remote operation".into()),
        }
        Ok(view(&state))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn suspend_and_clock_changes_cannot_extend_pairing_or_client_expiry() {
        let mut access = Access::default();
        access.new_code().unwrap();
        let code = access.pairing.as_ref().unwrap().code.clone();
        access.pairing.as_mut().unwrap().created.wall =
            SystemTime::now() - Duration::from_secs(121);
        assert!(
            access.pair(&code, "Phone").is_err(),
            "wall clock includes sleep when monotonic time does not"
        );
        access.new_code().unwrap();
        let code = access.pairing.as_ref().unwrap().code.clone();
        let token = access.pair(&code, "Phone").unwrap();
        let guard = access.authorize(&token).unwrap();
        access.clients[0].created.wall = SystemTime::now() - Duration::from_secs(86401);
        assert!(access.authorize(&token).is_err());
        assert!(!guard.load(Ordering::Acquire));
        let age = Age {
            monotonic: Instant::now(),
            wall: SystemTime::now() + Duration::from_secs(60),
        };
        assert_eq!(age.elapsed(), Duration::MAX);
    }
    #[test]
    fn real_tls_listener_authenticates_ip_origin_pairing_and_revocation() {
        use rustls::pki_types::pem::PemObject;
        use std::io::BufRead;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let host = listener.local_addr().unwrap().to_string();
        let (server, _, pem) = certificate("127.0.0.1").unwrap();
        let mut roots = rustls::RootCertStore::empty();
        roots
            .add(rustls::pki_types::CertificateDer::from_pem_slice(pem.as_bytes()).unwrap())
            .unwrap();
        let client = Arc::new(
            rustls::ClientConfig::builder_with_provider(Arc::new(
                rustls::crypto::ring::default_provider(),
            ))
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_root_certificates(roots)
            .with_no_client_auth(),
        );
        let active = Arc::new(AtomicBool::new(true));
        let access = Arc::new(Mutex::new(Access::default()));
        access.lock().unwrap().new_code().unwrap();
        let code = access
            .lock()
            .unwrap()
            .pairing
            .as_ref()
            .unwrap()
            .code
            .clone();
        serve(
            listener,
            Arc::new(server),
            active.clone(),
            access.clone(),
            host.clone(),
            Arc::new(|_, guard| {
                assert!(guard.load(Ordering::Acquire));
                Ok(json!({"connected":false,"requested":false,"brightness":42,"mode":"solid"}))
            }),
        )
        .unwrap();
        let call = |path: &str, body: Value, cookie: &str, origin: &str| {
            let socket = TcpStream::connect(&host).unwrap();
            socket
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            socket
                .set_write_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let connection = rustls::ClientConnection::new(
                client.clone(),
                rustls::pki_types::ServerName::try_from("127.0.0.1").unwrap(),
            )
            .unwrap();
            let mut stream = rustls::StreamOwned::new(connection, socket);
            let body = body.to_string();
            write!(stream,"POST {path} HTTP/1.1\r\nHost: {host}\r\nOrigin: {origin}\r\nContent-Type: application/json\r\nCookie: {cookie}\r\nContent-Length: {}\r\n\r\n{body}",body.len()).unwrap();
            stream.flush().unwrap();
            let mut reader = std::io::BufReader::new(stream);
            let mut head = String::new();
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                head.push_str(&line);
                if line == "\r\n" {
                    break;
                }
            }
            let length = head
                .lines()
                .find_map(|l| l.strip_prefix("Content-Length: "))
                .unwrap()
                .parse::<usize>()
                .unwrap();
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            (head, serde_json::from_slice::<Value>(&body).ok())
        };
        let origin = format!("https://{host}");
        let command = json!({"version":1,"command":{"command":"status"}});
        assert!(call("/v1/command", command.clone(), "", &origin)
            .0
            .starts_with("HTTP/1.1 401"));
        assert!(call(
            "/pair",
            json!({"code":code,"name":"Phone"}),
            "",
            "https://wrong.invalid"
        )
        .0
        .starts_with("HTTP/1.1 403"));
        let paired = call("/pair", json!({"code":code,"name":"Phone"}), "", &origin);
        assert!(paired.0.starts_with("HTTP/1.1 200"));
        let cookie = paired
            .0
            .lines()
            .find_map(|l| l.strip_prefix("Set-Cookie: "))
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        let authorized = call("/v1/command", command.clone(), cookie, &origin);
        assert_eq!(authorized.1.unwrap()["data"]["brightness"], 42);
        access.lock().unwrap().revoke(None);
        assert!(call("/v1/command", command, cookie, &origin)
            .0
            .starts_with("HTTP/1.1 401"));
        active.store(false, Ordering::Release);
    }
    #[test]
    fn pairing_is_bounded_single_use_and_revokes_existing_guards() {
        let mut access = Access::default();
        access.new_code().unwrap();
        let code = access.pairing.as_ref().unwrap().code.clone();
        let token = access.pair(&code, "Phone").unwrap();
        assert!(access.pair(&code, "Replay").is_err());
        assert_ne!(access.clients[0].hash, token.as_bytes());
        let guard = access.authorize(&token).unwrap();
        let id = access.clients[0].id.clone();
        access.revoke(Some(&id));
        assert!(!guard.load(Ordering::Acquire));
        assert!(access.authorize(&token).is_err());
        access.new_code().unwrap();
        for _ in 0..5 {
            assert!(access.pair("wrong", "Phone").is_err());
        }
        let code = access.pairing.as_ref().unwrap().code.clone();
        assert!(access.pair(&code, "Phone").is_err());
        access.new_code().unwrap();
        access.pairing.as_mut().unwrap().created.monotonic =
            Instant::now() - Duration::from_secs(121);
        let code = access.pairing.as_ref().unwrap().code.clone();
        assert!(access.pair(&code, "Phone").is_err());
    }
    #[test]
    fn remote_cannot_grant_capture_or_read_private_snapshot_fields() {
        assert!(!private("0.0.0.0".parse().unwrap()));
        assert!(!private("8.8.8.8".parse().unwrap()));
        assert!(private("192.168.1.2".parse().unwrap()));
        assert!(!permitted(&Command::Power {
            state: dynamiclight_control::Power::On,
            allow_capture: true
        }));
        assert!(!permitted(&Command::Routine {
            action: dynamiclight_control::RoutineControl::Start
        }));
        let public = public_result(
            &Command::Status {},
            json!({"colors":[1,2,3],"captureSource":"private","error":"private/path","pauseReason":"locked"}),
        );
        assert!(public.get("colors").is_none() && public.get("error").is_none());
        assert_eq!(public["paused"], true);
    }
}
