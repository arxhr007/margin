//! `margin settings`: a small local web server that serves the settings page and reads/writes
//! settings.json. It only listens on 127.0.0.1 and every request needs a random key that is part of
//! the printed link, so other programs and web pages cannot change your settings.

use std::io::{self, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::Duration;

use crate::settings::{self, Settings};

const PAGE: &str = include_str!("settings.html");
const MAX_HEADER: usize = 32 * 1024;
const MAX_BODY: usize = 1024 * 1024;

pub fn run(args: &[String]) {
    let mut port: u16 = 0;
    let mut open_browser = true;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--port" => port = it.next().and_then(|p| p.parse().ok()).unwrap_or(0),
            "--no-open" => open_browser = false,
            other => {
                eprintln!("unknown option {other}\nusage: margin settings [--port N] [--no-open]");
                std::process::exit(2);
            }
        }
    }

    let listener = match TcpListener::bind(("127.0.0.1", port)) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("could not start the settings page: {e}");
            std::process::exit(1);
        }
    };
    let port = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    let token = random_token();
    let url = format!("http://127.0.0.1:{port}/?t={token}");
    println!("Margin settings are open at:\n  {url}\nPress Ctrl+C here when you are done.");
    if open_browser {
        let _ = std::process::Command::new("xdg-open")
            .arg(&url)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }

    let ctx = Context { token, port, store: &DiskStore };
    for stream in listener.incoming().flatten() {
        let _ = handle(stream, &ctx);
    }
}

/// 128 random bits as hex, from the kernel.
fn random_token() -> String {
    let mut bytes = [0u8; 16];
    let ok = std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).is_ok();
    if !ok {
        // /dev/urandom is always there on Linux; this only guards against a broken sandbox
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        bytes = (t.as_nanos() ^ ((std::process::id() as u128) << 64)).to_le_bytes();
    }
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

// ---------------------------------------------------------------- HTTP

pub trait Store {
    fn load(&self) -> Settings;
    fn save(&self, s: &Settings) -> Result<(), String>;
}

struct DiskStore;

impl Store for DiskStore {
    fn load(&self) -> Settings {
        settings::load()
    }
    fn save(&self, s: &Settings) -> Result<(), String> {
        settings::try_save(s).map_err(|e| e.to_string())
    }
}

pub struct Context<'a> {
    pub token: String,
    pub port: u16,
    pub store: &'a dyn Store,
}

#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub query: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Request {
    fn header(&self, name: &str) -> Option<&str> {
        self.headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
}

#[derive(Debug)]
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
}

fn reply(status: u16, content_type: &'static str, body: impl Into<Vec<u8>>) -> Response {
    Response { status, content_type, body: body.into() }
}

fn text(status: u16, message: &str) -> Response {
    reply(status, "text/plain; charset=utf-8", message)
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        _ => "Error",
    }
}

fn query_param<'a>(query: &'a str, name: &str) -> Option<&'a str> {
    query.split('&').find_map(|kv| kv.strip_prefix(name)?.strip_prefix('='))
}

/// Decides the answer to one request. Pure apart from the settings store, so it is unit-tested.
pub fn route(req: &Request, ctx: &Context) -> Response {
    // Only our own page may talk to us: the right Host blocks DNS rebinding, the right Origin blocks other sites.
    let hosts = [format!("127.0.0.1:{}", ctx.port), format!("localhost:{}", ctx.port)];
    if !req.header("host").is_some_and(|h| hosts.iter().any(|a| a == h)) {
        return text(403, "wrong host");
    }
    if let Some(origin) = req.header("origin") {
        if !hosts.iter().any(|h| origin == format!("http://{h}")) {
            return text(403, "wrong origin");
        }
    }

    let authorised_page = query_param(&req.query, "t") == Some(ctx.token.as_str());
    let authorised_api = req.header("x-margin-token") == Some(ctx.token.as_str());

    match (req.method.as_str(), req.path.as_str()) {
        ("GET", "/") | ("GET", "/index.html") => {
            if !authorised_page {
                return text(403, "open the link that `margin settings` printed (it contains a key)");
            }
            reply(200, "text/html; charset=utf-8", PAGE)
        }
        ("GET", "/api/settings") => {
            if !authorised_api {
                return text(403, "missing key");
            }
            match serde_json::to_vec(&ctx.store.load()) {
                Ok(json) => reply(200, "application/json", json),
                Err(e) => text(500, &e.to_string()),
            }
        }
        ("PUT", "/api/settings") => {
            if !authorised_api {
                return text(403, "missing key");
            }
            let Ok(body) = std::str::from_utf8(&req.body) else { return text(400, "body is not UTF-8") };
            let mut incoming = match settings::parse(body) {
                Ok(s) => s,
                Err(e) => return text(400, &format!("not valid settings: {e}")),
            };
            incoming.normalize();
            match ctx.store.save(&incoming) {
                Ok(()) => reply(200, "application/json", &b"{\"ok\":true}"[..]),
                Err(e) => text(500, &format!("could not save: {e}")),
            }
        }
        ("GET", "/api/info") => {
            if !authorised_api {
                return text(403, "missing key");
            }
            let json = format!(
                "{{\"version\":\"{}\",\"configPath\":{}}}",
                env!("CARGO_PKG_VERSION"),
                serde_json::to_string(&settings::file_path().display().to_string()).unwrap_or_default()
            );
            reply(200, "application/json", json)
        }
        (_, "/") | (_, "/api/settings") | (_, "/api/info") => text(405, "method not allowed"),
        _ => text(404, "not found"),
    }
}

fn handle(mut stream: TcpStream, ctx: &Context) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let response = match read_request(&mut stream)? {
        Some(req) => route(&req, ctx),
        None => text(400, "bad request"),
    };
    write_response(&mut stream, &response)
}

fn write_response(stream: &mut TcpStream, r: &Response) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\n\
         X-Content-Type-Options: nosniff\r\nReferrer-Policy: no-referrer\r\n\
         Content-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; connect-src 'self'\r\n\
         Connection: close\r\n\r\n",
        r.status,
        reason(r.status),
        r.content_type,
        r.body.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(&r.body)?;
    stream.flush()
}

/// Reads one HTTP/1.1 request. `None` for anything malformed or too large.
fn read_request(stream: &mut TcpStream) -> io::Result<Option<Request>> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = find(&buf, b"\r\n\r\n") {
            break pos;
        }
        if buf.len() > MAX_HEADER {
            return Ok(None);
        }
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let (Some(method), Some(target)) = (first.next(), first.next()) else { return Ok(None) };
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    let headers: Vec<(String, String)> = lines
        .filter_map(|l| l.split_once(':'))
        .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_string()))
        .collect();

    let length: usize = headers.iter().find(|(k, _)| k == "content-length").and_then(|(_, v)| v.parse().ok()).unwrap_or(0);
    if length > MAX_BODY {
        return Ok(None);
    }
    let mut body = buf[header_end + 4..].to_vec();
    while body.len() < length {
        let n = stream.read(&mut chunk)?;
        if n == 0 {
            return Ok(None);
        }
        body.extend_from_slice(&chunk[..n]);
    }
    body.truncate(length);
    Ok(Some(Request { method: method.to_string(), path: path.to_string(), query: query.to_string(), headers, body }))
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Memory(Mutex<Settings>);

    impl Store for Memory {
        fn load(&self) -> Settings {
            self.0.lock().unwrap().clone()
        }
        fn save(&self, s: &Settings) -> Result<(), String> {
            *self.0.lock().unwrap() = s.clone();
            Ok(())
        }
    }

    fn ctx(store: &Memory) -> Context<'_> {
        Context { token: "secret".into(), port: 4242, store }
    }

    fn req(method: &str, path: &str, query: &str, headers: &[(&str, &str)], body: &str) -> Request {
        let mut h: Vec<(String, String)> = vec![("host".into(), "127.0.0.1:4242".into())];
        h.extend(headers.iter().map(|(k, v)| (k.to_string(), v.to_string())));
        Request { method: method.into(), path: path.into(), query: query.into(), headers: h, body: body.as_bytes().to_vec() }
    }

    #[test]
    fn the_page_needs_the_key_in_the_link() {
        let store = Memory(Mutex::new(Settings::default()));
        assert_eq!(route(&req("GET", "/", "", &[], ""), &ctx(&store)).status, 403);
        assert_eq!(route(&req("GET", "/", "t=wrong", &[], ""), &ctx(&store)).status, 403);
        let ok = route(&req("GET", "/", "t=secret", &[], ""), &ctx(&store));
        assert_eq!(ok.status, 200);
        assert!(ok.content_type.starts_with("text/html"));
        assert!(String::from_utf8_lossy(&ok.body).contains("Margin"));
    }

    #[test]
    fn the_api_needs_the_key_header() {
        let store = Memory(Mutex::new(Settings::default()));
        assert_eq!(route(&req("GET", "/api/settings", "t=secret", &[], ""), &ctx(&store)).status, 403, "a key in the URL is not enough for the API");
        assert_eq!(route(&req("GET", "/api/settings", "", &[("x-margin-token", "nope")], ""), &ctx(&store)).status, 403);
        let ok = route(&req("GET", "/api/settings", "", &[("x-margin-token", "secret")], ""), &ctx(&store));
        assert_eq!(ok.status, 200);
        let v: serde_json::Value = serde_json::from_slice(&ok.body).unwrap();
        assert_eq!(v["activation"], "slideIn");
        assert_eq!(v["sections"]["left"]["slide"], "volume");
    }

    #[test]
    fn saving_settings_round_trips_and_normalizes() {
        let store = Memory(Mutex::new(Settings::default()));
        let body = r#"{"speed":"fast","edgeDepth":9.0,"sections":{"top":{"slide":"custom","forward":"Alt+Tab","back":"Alt+Shift+Tab","holdModifiers":true}}}"#;
        let put = route(&req("PUT", "/api/settings", "", &[("x-margin-token", "secret")], body), &ctx(&store));
        assert_eq!(put.status, 200);
        let saved = store.load();
        assert_eq!(saved.edge_depth, 0.25, "normalized");
        assert_eq!(saved.section(settings::Section::Top).forward.as_deref(), Some("Alt+Tab"));
        assert!(saved.section(settings::Section::Top).hold_modifiers);
    }

    #[test]
    fn bad_bodies_are_rejected_and_nothing_is_saved() {
        let store = Memory(Mutex::new(Settings::default()));
        for body in ["not json", r#"{"speed":"warp"}"#, r#"{"sections":5}"#] {
            let r = route(&req("PUT", "/api/settings", "", &[("x-margin-token", "secret")], body), &ctx(&store));
            assert_eq!(r.status, 400, "{body}");
        }
        assert_eq!(store.load(), Settings::default());
    }

    #[test]
    fn other_hosts_and_other_sites_are_refused() {
        let store = Memory(Mutex::new(Settings::default()));
        // DNS rebinding: the browser sends the attacker's host name
        let mut rebinding = req("GET", "/api/settings", "", &[("x-margin-token", "secret")], "");
        rebinding.headers[0].1 = "evil.example:4242".into();
        assert_eq!(route(&rebinding, &ctx(&store)).status, 403);
        // a page on another site
        let cross = req("PUT", "/api/settings", "", &[("x-margin-token", "secret"), ("origin", "https://evil.example")], "{}");
        assert_eq!(route(&cross, &ctx(&store)).status, 403);
        // our own page is fine
        let own = req("GET", "/api/settings", "", &[("x-margin-token", "secret"), ("origin", "http://127.0.0.1:4242")], "");
        assert_eq!(route(&own, &ctx(&store)).status, 200);
        let localhost = Request { headers: vec![("host".into(), "localhost:4242".into()), ("x-margin-token".into(), "secret".into())], ..req("GET", "/api/settings", "", &[], "") };
        assert_eq!(route(&localhost, &ctx(&store)).status, 200);
    }

    #[test]
    fn unknown_paths_and_methods() {
        let store = Memory(Mutex::new(Settings::default()));
        assert_eq!(route(&req("GET", "/nope", "t=secret", &[], ""), &ctx(&store)).status, 404);
        assert_eq!(route(&req("DELETE", "/api/settings", "", &[("x-margin-token", "secret")], ""), &ctx(&store)).status, 405);
    }

    #[test]
    fn query_parameter_lookup() {
        assert_eq!(query_param("a=1&t=xyz&b=2", "t"), Some("xyz"));
        assert_eq!(query_param("tt=1", "t"), None);
        assert_eq!(query_param("", "t"), None);
    }

    #[test]
    fn tokens_are_32_hex_characters_and_differ() {
        let (a, b) = (random_token(), random_token());
        assert_eq!(a.len(), 32);
        assert!(a.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(a, b);
    }
}
