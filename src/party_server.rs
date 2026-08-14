use crate::search::search_youtube;
use crate::security::{sanitize_display_text_limited, valid_youtube_id};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc::Sender};
use std::thread::JoinHandle;

const MAX_REQUEST: usize = 16 * 1024;

pub struct PartyServer {
    stop: Arc<AtomicBool>,
    wake: std::net::SocketAddr,
    worker: Option<JoinHandle<()>>,
    pub url: String,
    pub access_code: String,
}

impl PartyServer {
    pub fn start_automatic(queue: Sender<(String, String)>) -> Result<Self, String> {
        Self::start(generate_access_code()?, queue)
    }

    pub fn start(password: String, queue: Sender<(String, String)>) -> Result<Self, String> {
        if password.chars().count() < 8 {
            return Err("CREST_PARTY_PASSWORD must contain at least 8 characters.".into());
        }
        let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, 8765))
            .or_else(|_| TcpListener::bind((Ipv4Addr::UNSPECIFIED, 0)))
            .map_err(|error| format!("could not open a local Party Mode port: {error}"))?;
        Self::start_with_listener(password, queue, listener)
    }

    fn start_with_listener(
        password: String,
        queue: Sender<(String, String)>,
        listener: TcpListener,
    ) -> Result<Self, String> {
        listener.set_nonblocking(true).map_err(|e| e.to_string())?;
        let wake = listener.local_addr().map_err(|e| e.to_string())?;
        let ip = local_ip().unwrap_or(Ipv4Addr::LOCALHOST);
        let url = format!("http://{ip}:{}", wake.port());
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let worker_password = password.clone();
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, peer)) if same_lan(peer.ip(), ip) => {
                        let password = worker_password.clone();
                        let queue = queue.clone();
                        std::thread::spawn(move || handle(stream, &password, &queue));
                    }
                    Ok((mut stream, _)) => {
                        respond(&mut stream, "403 Forbidden", "text/plain", "Forbidden")
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(50));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            stop,
            wake,
            worker: Some(worker),
            url,
            access_code: password,
        })
    }
}

fn generate_access_code() -> Result<String, String> {
    let mut random = [0_u8; 10];
    getrandom::fill(&mut random)
        .map_err(|error| format!("could not generate a Party Mode access code: {error}"))?;
    const ALPHABET: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
    Ok(random
        .iter()
        .map(|byte| ALPHABET[*byte as usize % ALPHABET.len()] as char)
        .collect())
}

fn same_lan(peer: std::net::IpAddr, local: Ipv4Addr) -> bool {
    match peer {
        std::net::IpAddr::V4(peer) => {
            peer.is_loopback() || peer.octets()[..3] == local.octets()[..3]
        }
        _ => false,
    }
}

fn local_ip() -> Option<Ipv4Addr> {
    let socket = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    socket.connect((Ipv4Addr::new(192, 0, 2, 1), 80)).ok()?;
    match socket.local_addr().ok()?.ip() {
        std::net::IpAddr::V4(ip) => Some(ip),
        _ => None,
    }
}

fn handle(mut stream: TcpStream, password: &str, queue: &Sender<(String, String)>) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
    let mut bytes = vec![0; MAX_REQUEST];
    let Ok(length) = stream.read(&mut bytes) else {
        return;
    };
    bytes.truncate(length);
    let request = String::from_utf8_lossy(&bytes);
    let first = request.lines().next().unwrap_or_default();
    let body = request
        .split_once("\r\n\r\n")
        .map(|(_, body)| body)
        .unwrap_or("");
    let fields = form_fields(body);
    if first == "GET / HTTP/1.1" {
        respond(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            &page("", ""),
        );
    } else if first == "POST /search HTTP/1.1"
        && fields
            .get("password")
            .is_some_and(|v| constant_time_eq(v, password))
    {
        let query = fields.get("q").map(String::as_str).unwrap_or("");
        let results = search_youtube(query).unwrap_or_default();
        let rows = results.into_iter().map(|(title, id)| format!("<form method=post action=/queue><input type=hidden name=password value=\"{}\"><input type=hidden name=id value=\"{}\"><input type=hidden name=title value=\"{}\"><button>Add</button> {}</form>", html(password), html(&id), html(&title), html(&title))).collect::<String>();
        respond(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            &page(password, &rows),
        );
    } else if first == "POST /queue HTTP/1.1"
        && fields
            .get("password")
            .is_some_and(|v| constant_time_eq(v, password))
    {
        let id = fields.get("id").cloned().unwrap_or_default();
        let title = sanitize_display_text_limited(
            fields
                .get("title")
                .map(String::as_str)
                .unwrap_or("Guest request"),
            512,
        );
        if valid_youtube_id(&id) {
            // Send the identifier, not the YouTube webpage URL. The main event loop
            // resolves it through the same playable-audio pipeline as local search.
            let _ = queue.send((title, id));
        }
        respond(
            &mut stream,
            "200 OK",
            "text/html; charset=utf-8",
            &page(password, "<p>Added to the queue.</p>"),
        );
    } else {
        respond(
            &mut stream,
            "401 Unauthorized",
            "text/html; charset=utf-8",
            &page("", "<p>Incorrect password.</p>"),
        );
    }
}

fn page(password: &str, content: &str) -> String {
    format!(
        "<!doctype html><meta name=viewport content='width=device-width'><title>Crest Party</title><style>body{{font:18px system-ui;max-width:700px;margin:3rem auto;padding:1rem;background:#111;color:#eee}}input,button{{font:inherit;padding:.7rem;margin:.3rem}}form{{margin:1rem 0}}</style><h1>Crest Party</h1><form method=post action=/search><input type=password name=password value=\"{}\" placeholder=Password required><input name=q maxlength=200 required placeholder='Search YouTube'><button>Search</button></form>{content}",
        html(password)
    )
}
fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &str) {
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'; style-src 'unsafe-inline'\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}
fn constant_time_eq(left: &str, right: &str) -> bool {
    let mut diff = left.len() ^ right.len();
    for (a, b) in left.bytes().zip(right.bytes()) {
        diff |= (a ^ b) as usize;
    }
    diff == 0
}
fn html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
fn form_fields(body: &str) -> std::collections::HashMap<String, String> {
    body.split('&')
        .filter_map(|part| {
            let (k, v) = part.split_once('=')?;
            Some((url_decode(k), url_decode(v)))
        })
        .collect()
}
fn url_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'+' {
            out.push(b' ');
            i += 1;
        } else if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&value[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl Drop for PartyServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let _ = TcpStream::connect_timeout(&self.wake, std::time::Duration::from_millis(100));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_decoder_handles_phone_search_text() {
        let fields = form_fields("password=ABCD1234&q=Daft+Punk+%26+Pharrell");
        assert_eq!(fields.get("password").map(String::as_str), Some("ABCD1234"));
        assert_eq!(
            fields.get("q").map(String::as_str),
            Some("Daft Punk & Pharrell")
        );
    }

    #[test]
    fn access_codes_are_long_unambiguous_and_random() {
        let first = generate_access_code().expect("code generation should work");
        let second = generate_access_code().expect("code generation should work");
        assert_eq!(first.len(), 10);
        assert!(
            first
                .bytes()
                .all(|byte| b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789".contains(&byte))
        );
        assert_ne!(first, second);
    }

    #[test]
    fn same_lan_accepts_loopback_and_matching_ipv4_subnet() {
        let local = Ipv4Addr::new(192, 168, 7, 20);
        assert!(same_lan("127.0.0.1".parse().unwrap(), local));
        assert!(same_lan("192.168.7.99".parse().unwrap(), local));
        assert!(!same_lan("192.168.8.99".parse().unwrap(), local));
    }
}
