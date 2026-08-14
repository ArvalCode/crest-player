use crate::security::{
    bounded_output, external_command, external_command_path, sanitize_display_text_limited,
    valid_media_url,
};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use std::process::{Child, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::{
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream, UdpSocket},
    sync::Mutex,
    thread::JoinHandle,
};

const DISCOVERY_OUTPUT_LIMIT: usize = 256 * 1024;
const RELAY_REQUEST_LIMIT: usize = 8 * 1024;
const PYATV_BOOTSTRAP: &str = "import asyncio,sys; asyncio.set_event_loop(asyncio.new_event_loop()); from pyatv.scripts.atvremote import main; sys.exit(main())";
const PYATV_IMPORT_CHECK: &str = "import pyatv";
fn airplay_command() -> std::process::Command {
    let Some(launcher) = airplay_launcher_path() else {
        return external_command("atvremote");
    };
    let interpreter = launcher_interpreter(&launcher);
    if let Some(interpreter) = interpreter {
        let mut command = std::process::Command::new(interpreter);
        command.args(["-E", "-c", PYATV_BOOTSTRAP]);
        command
    } else {
        std::process::Command::new(launcher)
    }
}

fn launcher_interpreter(launcher: &std::path::Path) -> Option<std::path::PathBuf> {
    std::fs::read_to_string(launcher)
        .ok()
        .and_then(|contents| contents.lines().next().map(str::to_string))
        .and_then(|line| line.strip_prefix("#!").map(str::to_string))
        .and_then(|line| line.split_whitespace().next().map(std::path::PathBuf::from))
        .filter(|path| path.is_absolute() && path.is_file())
}

fn airplay_helper_works() -> bool {
    let Some(launcher) = airplay_launcher_path() else {
        return false;
    };
    if let Some(interpreter) = launcher_interpreter(&launcher) {
        let mut command = std::process::Command::new(interpreter);
        command.args(["-E", "-c", PYATV_IMPORT_CHECK]);
        return bounded_output(command, 16 * 1024).is_ok_and(|output| output.status.success());
    }
    let mut command = std::process::Command::new(launcher);
    command.arg("--version");
    bounded_output(command, 16 * 1024).is_ok_and(|output| output.status.success())
}

fn airplay_launcher_path() -> Option<std::path::PathBuf> {
    external_command_path("atvremote").or_else(|| {
        let home = dirs::home_dir()?;
        #[cfg(windows)]
        let candidates = [
            home.join(".local/bin/atvremote.exe"),
            home.join(".local/share/pipx/venvs/pyatv/Scripts/atvremote.exe"),
        ];
        #[cfg(not(windows))]
        let candidates = [
            home.join(".local/bin/atvremote"),
            home.join(".local/share/pipx/venvs/pyatv/bin/atvremote"),
        ];
        candidates
            .into_iter()
            .find(|candidate| candidate.is_file())
            .and_then(|candidate| candidate.canonicalize().ok())
    })
}

fn select_airplay_device(command: &mut std::process::Command, device: &str) {
    if device.parse::<IpAddr>().is_ok() {
        command.args(["-s", device]);
    } else {
        command.args(["-n", device]);
    }
}

struct SonosRelay {
    stop: Arc<AtomicBool>,
    ffmpeg: Arc<Mutex<Option<Child>>>,
    started_at: Arc<Mutex<Option<std::time::Instant>>>,
    wake: SocketAddr,
    worker: Option<JoinHandle<()>>,
}

impl SonosRelay {
    fn start(source: &str, speaker: Ipv4Addr) -> Result<(Self, String), String> {
        let route = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))
            .and_then(|socket| {
                socket.connect((speaker, 1400))?;
                socket.local_addr()
            })
            .map_err(|error| format!("could not select a Sonos network route: {error}"))?;
        let local_ip = match route.ip() {
            IpAddr::V4(address) => address,
            _ => return Err("Sonos streaming needs an IPv4 network route.".to_string()),
        };
        let listener = TcpListener::bind((local_ip, 0))
            .map_err(|error| format!("could not open the Sonos stream relay: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("could not configure the Sonos stream relay: {error}"))?;
        let wake = listener
            .local_addr()
            .map_err(|error| format!("could not read the Sonos relay address: {error}"))?;
        let stop = Arc::new(AtomicBool::new(false));
        let ffmpeg = Arc::new(Mutex::new(None));
        let started_at = Arc::new(Mutex::new(None));
        let worker_stop = Arc::clone(&stop);
        let worker_ffmpeg = Arc::clone(&ffmpeg);
        let worker_started_at = Arc::clone(&started_at);
        let source = source.to_string();
        let worker = std::thread::spawn(move || {
            while !worker_stop.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, peer)) if peer.ip() == IpAddr::V4(speaker) => {
                        serve_sonos_stream(
                            stream,
                            &source,
                            &worker_stop,
                            &worker_ffmpeg,
                            &worker_started_at,
                        );
                    }
                    Ok((mut stream, _)) => {
                        let _ = stream.write_all(
                            b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                        );
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(25));
                    }
                    Err(_) => break,
                }
            }
        });
        let url = format!("http://{wake}/stream.mp3");
        Ok((
            Self {
                stop,
                ffmpeg,
                started_at,
                wake,
                worker: Some(worker),
            },
            url,
        ))
    }

    fn started_at(&self) -> Option<std::time::Instant> {
        self.started_at.lock().ok().and_then(|started| *started)
    }

    fn stop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Ok(mut child) = self.ffmpeg.lock()
            && let Some(child) = child.as_mut()
        {
            let _ = child.kill();
        }
        let _ = TcpStream::connect_timeout(&self.wake, std::time::Duration::from_millis(100));
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        if let Ok(mut child) = self.ffmpeg.lock()
            && let Some(mut child) = child.take()
        {
            let _ = child.wait();
        }
    }
}

impl Drop for SonosRelay {
    fn drop(&mut self) {
        self.stop();
    }
}

fn serve_sonos_stream(
    mut stream: TcpStream,
    source: &str,
    stop: &AtomicBool,
    ffmpeg_slot: &Mutex<Option<Child>>,
    started_at: &Mutex<Option<std::time::Instant>>,
) {
    let _ = stream.set_read_timeout(Some(std::time::Duration::from_secs(2)));
    let mut request = [0u8; RELAY_REQUEST_LIMIT];
    let Ok(length) = stream.read(&mut request) else {
        return;
    };
    if length == request.len()
        || !request[..length].starts_with(b"GET /stream.mp3 ")
        || stop.load(Ordering::Acquire)
    {
        let _ = stream
            .write_all(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    }
    let mut command = external_command("ffmpeg");
    command.args([
        "-nostdin",
        "-loglevel",
        "error",
        "-i",
        source,
        "-vn",
        "-codec:a",
        "libmp3lame",
        "-b:a",
        "192k",
        "-f",
        "mp3",
        "pipe:1",
    ]);
    let Ok(mut child) = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return;
    };
    let Some(mut audio) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return;
    };
    if let Ok(mut slot) = ffmpeg_slot.lock() {
        *slot = Some(child);
    }
    if stream
        .write_all(
            b"HTTP/1.1 200 OK\r\nContent-Type: audio/mpeg\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
        )
        .is_ok()
    {
        if let Ok(mut started) = started_at.lock() {
            *started = Some(std::time::Instant::now());
        }
        let _ = std::io::copy(&mut audio, &mut stream);
    }
    if let Ok(mut slot) = ffmpeg_slot.lock()
        && let Some(mut child) = slot.take()
    {
        let _ = child.kill();
        let _ = child.wait();
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CastDevice {
    pub name: String,
    pub target: CastTarget,
}

pub struct DiscoveryResult {
    pub devices: Vec<CastDevice>,
    pub notice: Option<String>,
}

pub struct DiscoveryHandle {
    receiver: Receiver<DiscoveryResult>,
    cancelled: Arc<AtomicBool>,
}

impl DiscoveryHandle {
    pub fn try_recv(&self) -> Result<DiscoveryResult, mpsc::TryRecvError> {
        self.receiver.try_recv()
    }
}

impl Drop for DiscoveryHandle {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Release);
    }
}

pub fn start_discovery() -> DiscoveryHandle {
    let (sender, receiver) = mpsc::channel();
    let cancelled = Arc::new(AtomicBool::new(false));
    let worker_cancelled = Arc::clone(&cancelled);
    std::thread::spawn(move || {
        let (mut devices, airplay_notice) = discover_airplay();
        if worker_cancelled.load(Ordering::Acquire) {
            return;
        }
        let (sonos, sonos_notice) = discover_sonos(&worker_cancelled);
        devices.extend(sonos);
        let (bluetooth, bluetooth_notice) = discover_bluetooth(&worker_cancelled);
        devices.extend(bluetooth);
        devices.sort_by_key(|device| (device.target.protocol_order(), device.name.to_lowercase()));
        devices.dedup_by(|left, right| left.target == right.target);
        let notice = [airplay_notice, sonos_notice, bluetooth_notice]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" ");
        let notice = (!notice.is_empty()).then_some(notice);
        if !worker_cancelled.load(Ordering::Acquire) {
            let _ = sender.send(DiscoveryResult { devices, notice });
        }
    });
    DiscoveryHandle {
        receiver,
        cancelled,
    }
}

#[cfg(target_os = "linux")]
fn discover_bluetooth(cancelled: &AtomicBool) -> (Vec<CastDevice>, Option<String>) {
    if !command_works("bluetoothctl", &["--version"]) {
        return (
            Vec::new(),
            Some("Bluetooth search needs bluetoothctl (BlueZ).".to_string()),
        );
    }
    let mut scan = external_command("bluetoothctl");
    scan.args(["--timeout", "4", "scan", "on"]);
    let _ = bounded_output(scan, DISCOVERY_OUTPUT_LIMIT);
    if cancelled.load(Ordering::Acquire) {
        return (Vec::new(), None);
    }
    let mut command = external_command("bluetoothctl");
    command.arg("devices");
    let Ok(output) = bounded_output(command, DISCOVERY_OUTPUT_LIMIT) else {
        return (
            Vec::new(),
            Some("Bluetooth device search failed.".to_string()),
        );
    };
    let candidates = parse_bluetooth_devices(&String::from_utf8_lossy(&output.stdout));
    let devices = candidates
        .into_iter()
        .filter(|device| {
            if cancelled.load(Ordering::Acquire) {
                return false;
            }
            let CastTarget::Bluetooth(address) = &device.target else {
                return false;
            };
            let mut info = external_command("bluetoothctl");
            info.args(["info", address]);
            bounded_output(info, 32 * 1024).is_ok_and(|output| {
                let output = String::from_utf8_lossy(&output.stdout).to_lowercase();
                output.contains("audio sink")
                    || output.contains("audio-card")
                    || output.contains("0000110b-0000-1000-8000-00805f9b34fb")
            })
        })
        .collect();
    (devices, None)
}

#[cfg(not(target_os = "linux"))]
fn discover_bluetooth(_cancelled: &AtomicBool) -> (Vec<CastDevice>, Option<String>) {
    (
        Vec::new(),
        Some("Bluetooth speaker search currently requires Linux BlueZ.".to_string()),
    )
}

fn parse_bluetooth_devices(output: &str) -> Vec<CastDevice> {
    output
        .lines()
        .filter_map(|line| {
            let mut columns = line.trim().splitn(3, ' ');
            (columns.next()? == "Device").then_some(())?;
            let address = columns.next()?.trim();
            let name = sanitize_display_text_limited(columns.next()?.trim(), 128);
            let valid_address = address.len() == 17
                && address.bytes().enumerate().all(|(index, byte)| {
                    if index % 3 == 2 {
                        byte == b':'
                    } else {
                        byte.is_ascii_hexdigit()
                    }
                });
            (valid_address && valid_device_name(&name)).then(|| CastDevice {
                name: format!("{name}  ·  Bluetooth"),
                target: CastTarget::Bluetooth(address.to_string()),
            })
        })
        .collect()
}

fn discover_airplay() -> (Vec<CastDevice>, Option<String>) {
    let notice = match ensure_airplay_tools() {
        Ok(notice) => notice,
        Err(message) => return (Vec::new(), Some(message)),
    };
    let mut command = airplay_command();
    command.args([
        "--scan-timeout",
        "3",
        "--scan-protocols",
        "airplay,raop",
        "scan",
    ]);
    let Ok(output) = bounded_output(command, DISCOVERY_OUTPUT_LIMIT) else {
        return (Vec::new(), notice);
    };
    let devices = parse_airplay_devices(&String::from_utf8_lossy(&output.stdout));
    (devices, notice)
}

fn parse_airplay_devices(output: &str) -> Vec<CastDevice> {
    output
        .split("\n\n")
        .filter(|block| block.contains("Protocol: RAOP") && !block.contains("Pairing: Unsupported"))
        .filter_map(|block| {
            let name = block
                .lines()
                .find_map(|line| line.trim().strip_prefix("Name:"))?;
            let address = block
                .lines()
                .find_map(|line| line.trim().strip_prefix("Address:"))?
                .trim()
                .parse::<IpAddr>()
                .ok()?;
            let name = sanitize_display_text_limited(name.trim(), 128);
            valid_device_name(&name).then(|| CastDevice {
                name: format!("{name}  ·  AirPlay"),
                target: CastTarget::AirPlay(address.to_string()),
            })
        })
        .collect()
}

fn discover_sonos(cancelled: &AtomicBool) -> (Vec<CastDevice>, Option<String>) {
    let notice = match ensure_sonos_tools() {
        Ok(notice) => notice,
        Err(message) => return (Vec::new(), Some(message)),
    };
    // Refresh SoCo-CLI's cache, then parse its stable human-readable table.
    let mut discovery = external_command("sonos-discover");
    discovery.args(["-t", "32", "-n", "0.25"]);
    let _ = bounded_output(discovery, DISCOVERY_OUTPUT_LIMIT);
    if cancelled.load(Ordering::Acquire) {
        return (Vec::new(), notice);
    }
    let mut command = external_command("sonos-discover");
    command.arg("-p");
    let Ok(output) = bounded_output(command, DISCOVERY_OUTPUT_LIMIT) else {
        return (Vec::new(), notice);
    };
    (
        parse_sonos_names(&String::from_utf8_lossy(&output.stdout)),
        notice,
    )
}

fn command_works(name: &str, arguments: &[&str]) -> bool {
    let mut command = external_command(name);
    command.args(arguments);
    bounded_output(command, 16 * 1024)
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn ensure_airplay_tools() -> Result<Option<String>, String> {
    if airplay_helper_works() {
        return Ok(None);
    }
    ensure_pipx()?;
    let mut command = external_command("pipx");
    command.args(["install", "pyatv"]);
    let install_succeeded = bounded_output(command, DISCOVERY_OUTPUT_LIMIT)
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !airplay_helper_works() {
        return Err(
            "AirPlay helper setup failed because pyatv could not be imported. Run: pipx reinstall pyatv"
                .to_string(),
        );
    }
    Ok(Some(if install_succeeded {
        "Installed the AirPlay helper automatically; scanning RAOP receivers now.".to_string()
    } else {
        "The existing AirPlay helper was verified; scanning RAOP receivers now.".to_string()
    }))
}

fn ensure_pipx() -> Result<(), String> {
    if command_works("pipx", &["--version"]) {
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    {
        let mut command = external_command("sudo");
        command.args([
            "-n",
            "pacman",
            "-S",
            "--needed",
            "--noconfirm",
            "python-pipx",
        ]);
        if bounded_output(command, DISCOVERY_OUTPUT_LIMIT)
            .map(|output| output.status.success())
            .unwrap_or(false)
        {
            return Ok(());
        }
        Err("Casting setup needs pipx. Run once: sudo pacman -S --needed python-pipx".to_string())
    }
    #[cfg(not(target_os = "linux"))]
    Err("Casting setup needs pipx. Install pipx, then rescan.".to_string())
}

fn ensure_sonos_tools() -> Result<Option<String>, String> {
    if command_works("sonos-discover", &["--version"]) {
        return Ok(None);
    }

    ensure_pipx()?;

    let mut command = external_command("pipx");
    command.args(["install", "soco-cli"]);
    let install_succeeded = bounded_output(command, DISCOVERY_OUTPUT_LIMIT)
        .map(|output| output.status.success())
        .unwrap_or(false);
    if !command_works("sonos-discover", &["--version"]) {
        return Err(
            "Automatic SoCo-CLI installation failed. Run: pipx install soco-cli".to_string(),
        );
    }
    Ok(Some(if install_succeeded {
        "Installed the Sonos helper automatically; scanning the network now.".to_string()
    } else {
        "The existing Sonos helper was verified; scanning the network now.".to_string()
    }))
}

fn parse_sonos_names(output: &str) -> Vec<CastDevice> {
    output
        .lines()
        .map(str::trim)
        .filter_map(|line| {
            let columns = line.split_whitespace().collect::<Vec<_>>();
            let address = columns
                .iter()
                .position(|column| column.parse::<std::net::Ipv4Addr>().is_ok())?;
            let name = sanitize_display_text_limited(&columns[..address].join(" "), 128);
            valid_device_name(&name).then(|| CastDevice {
                name: format!("{name}  ·  Sonos"),
                target: CastTarget::Sonos(columns[address].to_string()),
            })
        })
        .collect()
}

fn valid_device_name(value: &str) -> bool {
    !value.is_empty()
        && !value.starts_with('-')
        && value.len() <= 128
        && !value.chars().any(char::is_control)
}

fn valid_sonos_target(value: &str) -> bool {
    value.parse::<std::net::Ipv4Addr>().is_ok() || valid_device_name(value)
}

pub fn draw_speakers_page(
    frame: &mut ratatui::Frame,
    devices: &[CastDevice],
    selected: usize,
    scanning: bool,
    current: &str,
    notice: Option<&str>,
    group: (&[CastTarget], u8),
) {
    let (active, volume) = group;
    let area = frame.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .title("Crest Player · Settings · Speakers");
    frame.render_widget(block, area);
    let inner = ratatui::layout::Rect {
        x: area.x.saturating_add(2),
        y: area.y.saturating_add(2),
        width: area.width.saturating_sub(4),
        height: area.height.saturating_sub(4),
    };
    let chunks = ratatui::layout::Layout::vertical([
        ratatui::layout::Constraint::Length(3),
        ratatui::layout::Constraint::Min(3),
        ratatui::layout::Constraint::Length(2),
    ])
    .split(inner);
    frame.render_widget(
        Paragraph::new(if let Some(notice) = notice {
            notice
        } else if scanning {
            "Checking casting support and scanning this Wi-Fi network…"
        } else if devices.is_empty() {
            "No compatible speakers found. Press R to scan again."
        } else {
            "Available speakers"
        })
        .style(Style::default().fg(Color::Gray)),
        chunks[0],
    );
    let items = devices
        .iter()
        .map(|device| {
            let marker = if active.contains(&device.target) {
                "●"
            } else {
                "○"
            };
            ListItem::new(Line::from(format!("{marker} {}", device.name)))
        })
        .collect::<Vec<_>>();
    let mut state = ListState::default().with_selected((!devices.is_empty()).then_some(selected));
    frame.render_stateful_widget(
        List::new(items)
            .highlight_style(Style::default().fg(Color::Black).bg(Color::Red))
            .highlight_symbol("› "),
        chunks[1],
        &mut state,
    );
    frame.render_widget(
        Paragraph::new(format!(
            "{current}  ·  Volume {volume}% · +/- adjust · ↑/↓ select · Enter toggle · R rescan · D disconnect all · Esc back"
        )),
        chunks[2],
    );
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CastTarget {
    AirPlay(String),
    Sonos(String),
    Bluetooth(String),
}

impl CastTarget {
    pub fn label(&self) -> String {
        match self {
            Self::AirPlay(device) => format!("AirPlay: {device}"),
            Self::Sonos(device) => format!("Sonos: {device}"),
            Self::Bluetooth(device) => format!("Bluetooth: {device}"),
        }
    }

    fn protocol_order(&self) -> u8 {
        match self {
            Self::AirPlay(_) => 0,
            Self::Sonos(_) => 1,
            Self::Bluetooth(_) => 2,
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum CastCommand {
    Connect(CastTarget),
    Off,
    Status,
}

impl CastCommand {
    pub fn parse(input: &str) -> Result<Self, String> {
        let mut words = input.split_whitespace();
        if words.next() != Some(":cast") {
            return Err(help());
        }
        match words.next() {
            Some("off") if words.next().is_none() => Ok(Self::Off),
            Some("status") if words.next().is_none() => Ok(Self::Status),
            Some(protocol @ ("airplay" | "sonos")) => {
                let device = words.collect::<Vec<_>>().join(" ");
                let valid = match protocol {
                    "airplay" => valid_device_name(&device),
                    _ => valid_sonos_target(&device),
                };
                if !valid {
                    return Err(help());
                }
                Ok(Self::Connect(match protocol {
                    "airplay" => CastTarget::AirPlay(device),
                    _ => CastTarget::Sonos(device),
                }))
            }
            _ => Err(help()),
        }
    }
}

pub fn help() -> String {
    "Casting: :cast airplay <device> · :cast sonos <room-or-IP> · :cast status · :cast off"
        .to_string()
}

pub struct Caster {
    targets: Vec<CastTarget>,
    sessions: Vec<CastSession>,
    volume: u8,
}

struct CastSession {
    child: Option<Child>,
    relay: Option<SonosRelay>,
    source_child: Option<Child>,
}

impl Caster {
    pub fn new() -> Self {
        Self {
            targets: Vec::new(),
            sessions: Vec::new(),
            volume: 50,
        }
    }

    pub fn needs_network_clock(&self) -> bool {
        let has_network = self
            .targets
            .iter()
            .any(|target| !matches!(target, CastTarget::Bluetooth(_)));
        let has_bluetooth = self
            .targets
            .iter()
            .any(|target| matches!(target, CastTarget::Bluetooth(_)));
        has_network && !has_bluetooth
    }

    pub fn status(&self) -> String {
        match self.targets.as_slice() {
            [] => "Casting is off.".to_string(),
            [target] => format!("Casting to {}.", target.label()),
            targets => format!("Casting to {} speakers.", targets.len()),
        }
    }

    pub fn is_waiting_for_stream(&self) -> bool {
        self.sessions.iter().any(|session| {
            session
                .relay
                .as_ref()
                .is_some_and(|relay| relay.started_at().is_none())
        })
    }

    pub fn stream_started_at(&self) -> Option<std::time::Instant> {
        self.sessions
            .iter()
            .filter_map(|session| session.relay.as_ref().and_then(SonosRelay::started_at))
            .max()
    }

    pub fn connect(&mut self, target: CastTarget) -> String {
        let label = target.label();
        if let Some(index) = self.targets.iter().position(|selected| selected == &target) {
            self.stop();
            self.targets.remove(index);
            format!("Removed {label} from the speaker group.")
        } else {
            self.stop();
            self.targets.push(target);
            self.apply_volume();
            format!(
                "Added {label}. The next track will use {} speaker(s).",
                self.targets.len()
            )
        }
    }

    pub fn targets(&self) -> &[CastTarget] {
        &self.targets
    }

    pub fn volume(&self) -> u8 {
        self.volume
    }

    pub fn adjust_volume(&mut self, change: i8) -> String {
        self.volume = if change.is_negative() {
            self.volume.saturating_sub(change.unsigned_abs())
        } else {
            self.volume.saturating_add(change as u8).min(100)
        };
        self.apply_volume();
        format!("Speaker group volume: {}%.", self.volume)
    }

    fn apply_volume(&self) {
        spawn_controls(volume_commands(&self.targets, self.volume), false);
    }

    pub fn off(&mut self) -> String {
        self.stop();
        self.targets.clear();
        "Casting disabled; playback will use this computer.".to_string()
    }

    pub fn play(&mut self, path: &str) -> Result<(), String> {
        self.stop_sessions();
        if self.targets.is_empty() {
            return Ok(());
        }
        let targets = self.targets.clone();
        let mut errors = Vec::new();
        for target in targets {
            match CastSession::play(target, path) {
                Ok(session) => self.sessions.push(session),
                Err(error) => errors.push(error),
            }
        }
        self.apply_volume();
        if self.sessions.is_empty() && !errors.is_empty() {
            Err(errors.join(" "))
        } else {
            Ok(())
        }
    }

    pub fn pause(&mut self) {
        self.control("pause");
    }

    pub fn resume(&mut self) {
        self.control("play");
    }

    pub fn seek_to(&self, position: std::time::Duration) {
        let commands = self
            .targets
            .iter()
            .map(|target| control_command(target, "seek", Some(position)))
            .collect();
        spawn_controls(commands, false);
    }

    pub fn stop(&mut self) {
        let commands = self
            .targets
            .iter()
            .map(|target| control_command(target, "stop", None))
            .collect();
        spawn_controls(commands, true);
        self.stop_sessions();
    }

    fn control(&self, action: &str) {
        let commands = self
            .targets
            .iter()
            .map(|target| control_command(target, action, None))
            .collect();
        spawn_controls(commands, false);
    }

    fn stop_sessions(&mut self) {
        for session in &mut self.sessions {
            session.stop();
        }
        self.sessions.clear();
    }
}

impl CastSession {
    fn play(target: CastTarget, path: &str) -> Result<Self, String> {
        let mut session = Self {
            child: None,
            relay: None,
            source_child: None,
        };
        if let CastTarget::AirPlay(device) = &target
            && valid_media_url(path)
        {
            let mut source = external_command("ffmpeg");
            let mut source = source
                .args([
                    "-nostdin",
                    "-loglevel",
                    "error",
                    "-i",
                    path,
                    "-vn",
                    "-codec:a",
                    "libmp3lame",
                    "-b:a",
                    "192k",
                    "-f",
                    "mp3",
                    "pipe:1",
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|error| {
                    format!("could not start the AirPlay stream converter: {error}")
                })?;
            let Some(audio) = source.stdout.take() else {
                let _ = source.kill();
                let _ = source.wait();
                return Err("AirPlay stream converter did not expose audio output.".to_string());
            };
            let mut command = airplay_command();
            select_airplay_device(&mut command, device);
            match command
                .arg("stream_file=-")
                .stdin(Stdio::from(audio))
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                Ok(child) => {
                    session.source_child = Some(source);
                    session.child = Some(child);
                    return Ok(session);
                }
                Err(error) => {
                    let _ = source.kill();
                    let _ = source.wait();
                    return Err(format!("could not start AirPlay streaming: {error}"));
                }
            }
        }
        let mut command = match target {
            CastTarget::AirPlay(device) => {
                let mut command = airplay_command();
                select_airplay_device(&mut command, &device);
                command.arg(format!("stream_file={path}"));
                command
            }
            CastTarget::Sonos(device) => {
                let mut command = external_command("sonos");
                if valid_media_url(path) {
                    let speaker = device.parse::<Ipv4Addr>().map_err(|_| {
                        "Select Sonos from Settings → Speakers before streaming.".to_string()
                    })?;
                    let (relay, url) = SonosRelay::start(path, speaker)?;
                    session.relay = Some(relay);
                    command.args(["-l", &device, "play_uri", &url]);
                } else {
                    command.args(["-l", &device, "play_file", path]);
                }
                command
            }
            CastTarget::Bluetooth(address) => {
                let mut command = external_command("bluetoothctl");
                command.args(["connect", &address]);
                command
            }
        };
        session.child = command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(Some)
            .map_err(|error| format!("could not start casting helper: {error}"))?;
        Ok(session)
    }

    fn stop(&mut self) {
        if let Some(mut relay) = self.relay.take() {
            relay.stop();
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(mut child) = self.source_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn control_command(
    target: &CastTarget,
    action: &str,
    position: Option<std::time::Duration>,
) -> std::process::Command {
    match target {
        CastTarget::AirPlay(device) => {
            let mut command = airplay_command();
            select_airplay_device(&mut command, device);
            if let Some(position) = position {
                command.arg(format!("set_position={:.3}", position.as_secs_f64()));
            } else {
                command.arg(action);
            }
            command
        }
        CastTarget::Sonos(device) => {
            let mut command = external_command("sonos");
            if let Some(position) = position {
                let total = position.as_secs();
                let timestamp = format!(
                    "{:02}:{:02}:{:02}",
                    total / 3600,
                    (total % 3600) / 60,
                    total % 60
                );
                command.args(["-l", device, "seek", &timestamp]);
            } else {
                command.args(["-l", device, action]);
            }
            command
        }
        CastTarget::Bluetooth(address) => {
            let mut command = external_command("bluetoothctl");
            if action == "stop" {
                command.args(["disconnect", address]);
            } else {
                command.args(["connect", address]);
            }
            command
        }
    }
}

fn volume_commands(targets: &[CastTarget], volume: u8) -> Vec<std::process::Command> {
    let mut commands = Vec::new();
    let mut local_volume_added = false;
    for target in targets {
        match target {
            CastTarget::AirPlay(device) => {
                let mut command = airplay_command();
                select_airplay_device(&mut command, device);
                command.arg(format!("set_volume={volume}"));
                commands.push(command);
            }
            CastTarget::Sonos(device) => {
                let mut command = external_command("sonos");
                command.args(["-l", device, "volume", &volume.to_string()]);
                commands.push(command);
            }
            CastTarget::Bluetooth(_) if !local_volume_added => {
                if let Some(command) = local_volume_command(volume) {
                    commands.push(command);
                }
                local_volume_added = true;
            }
            CastTarget::Bluetooth(_) => {}
        }
    }
    commands
}

#[cfg(target_os = "linux")]
fn local_volume_command(volume: u8) -> Option<std::process::Command> {
    if external_command_path("wpctl").is_some() {
        let mut command = external_command("wpctl");
        command.args(["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{volume}%")]);
        Some(command)
    } else if external_command_path("pactl").is_some() {
        let mut command = external_command("pactl");
        command.args(["set-sink-volume", "@DEFAULT_SINK@", &format!("{volume}%")]);
        Some(command)
    } else {
        None
    }
}

#[cfg(target_os = "macos")]
fn local_volume_command(volume: u8) -> Option<std::process::Command> {
    let mut command = external_command("osascript");
    command.args(["-e", &format!("set volume output volume {volume}")]);
    Some(command)
}

#[cfg(windows)]
fn local_volume_command(volume: u8) -> Option<std::process::Command> {
    let mut command = external_command("powershell");
    command.args([
        "-NoProfile",
        "-NonInteractive",
        "-Command",
        &format!(
            "$shell=New-Object -ComObject WScript.Shell; 1..50 | % {{$shell.SendKeys([char]174)}}; 1..{} | % {{$shell.SendKeys([char]175)}}",
            volume.div_ceil(2)
        ),
    ]);
    Some(command)
}

#[cfg(not(any(target_os = "linux", target_os = "macos", windows)))]
fn local_volume_command(_volume: u8) -> Option<std::process::Command> {
    None
}

fn spawn_controls(commands: Vec<std::process::Command>, wait: bool) {
    if wait {
        // Start every receiver command before waiting for any one receiver, so
        // a slow device cannot delay dispatch to the rest of the group.
        let mut children = commands
            .into_iter()
            .filter_map(|mut command| {
                command
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                    .ok()
            })
            .collect::<Vec<_>>();
        for child in &mut children {
            let _ = child.wait();
        }
    } else {
        for mut command in commands {
            std::thread::spawn(move || {
                let _ = command
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status();
            });
        }
    }
}

impl Drop for Caster {
    fn drop(&mut self) {
        if !self.sessions.is_empty() {
            self.stop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CastCommand, CastTarget, Caster};

    #[test]
    fn parses_device_names_with_spaces() {
        assert_eq!(
            CastCommand::parse(":cast airplay Living Room").unwrap(),
            CastCommand::Connect(CastTarget::AirPlay("Living Room".to_string()))
        );
        assert_eq!(
            CastCommand::parse(":cast sonos 192.168.1.40").unwrap(),
            CastCommand::Connect(CastTarget::Sonos("192.168.1.40".to_string()))
        );
    }

    #[test]
    fn rejects_missing_device() {
        assert!(CastCommand::parse(":cast airplay").is_err());
        assert!(CastCommand::parse(":cast unknown speaker").is_err());
        assert!(CastCommand::parse(":cast sonos --actions").is_err());
        assert!(CastCommand::parse(":cast airplay -n attacker").is_err());
    }

    #[test]
    fn parses_sonos_discovery_names() {
        let devices = super::parse_sonos_names(
            "Room/Zone Name    IP Address     Device Model\n\
             ----------------  -------------  ------------\n\
             Kitchen           192.168.1.40   One SL\n\
             Living Room       192.168.1.41   Beam\n",
        );
        assert_eq!(devices.len(), 2);
        assert_eq!(
            devices[0].target,
            CastTarget::Sonos("192.168.1.40".to_string())
        );
        assert_eq!(devices[1].name, "Living Room  ·  Sonos");
    }

    #[test]
    fn sanitizes_untrusted_discovery_names() {
        let devices =
            super::parse_sonos_names("Bad\u{1b}]52;c;secret\u{7} Room  192.168.1.40  One SL\n");
        assert_eq!(devices.len(), 1);
        assert!(!devices[0].name.contains('\u{1b}'));
        assert!(!devices[0].name.contains('\u{7}'));
        assert_eq!(devices[0].target, CastTarget::Sonos("192.168.1.40".into()));
    }

    #[test]
    fn airplay_discovery_keeps_only_usable_raop_receivers() {
        let devices = super::parse_airplay_devices(
            "Name: Living Room\nAddress: 192.168.1.81\nServices:\n - Protocol: RAOP, Port: 7000, Pairing: NotNeeded\n\n\
             Name: Television\nAddress: 192.168.1.90\nServices:\n - Protocol: AirPlay, Port: 7000, Pairing: Mandatory\n\n\
             Name: Laptop\nAddress: 192.168.1.91\nServices:\n - Protocol: RAOP, Port: 7000, Pairing: Unsupported\n",
        );
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Living Room  ·  AirPlay");
        assert_eq!(
            devices[0].target,
            CastTarget::AirPlay("192.168.1.81".to_string())
        );
    }

    #[test]
    fn target_labels_include_the_protocol() {
        assert_eq!(
            CastTarget::Sonos("192.168.1.40".to_string()).label(),
            "Sonos: 192.168.1.40"
        );
    }

    #[test]
    fn parses_only_well_formed_bluetooth_device_rows() {
        let devices = super::parse_bluetooth_devices(
            "Device AA:BB:CC:DD:EE:FF Living Room Speaker\nDevice invalid Keyboard\n",
        );
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].name, "Living Room Speaker  ·  Bluetooth");
        assert_eq!(
            devices[0].target,
            CastTarget::Bluetooth("AA:BB:CC:DD:EE:FF".into())
        );
    }

    #[test]
    fn speaker_protocol_order_is_airplay_sonos_bluetooth() {
        assert!(
            CastTarget::AirPlay("a".into()).protocol_order()
                < CastTarget::Sonos("s".into()).protocol_order()
        );
        assert!(
            CastTarget::Sonos("s".into()).protocol_order()
                < CastTarget::Bluetooth("b".into()).protocol_order()
        );
    }

    #[test]
    fn group_volume_is_clamped_to_a_safe_percentage_range() {
        let mut caster = Caster::new();
        assert_eq!(caster.volume(), 50);
        caster.adjust_volume(100);
        assert_eq!(caster.volume(), 100);
        caster.adjust_volume(-100);
        assert_eq!(caster.volume(), 0);
    }
}
