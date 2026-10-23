//! Discord Rich Presence over Discord's documented local IPC ("hard mode" in the official docs).
//! Protocol verified against the MIT-licensed discord/discord-rpc sources (deprecated by Discord in
//! favour of its newer SDKs, so this is labelled Experimental until tested against a live client).
//!
//! Privacy rules (PROJECT_SPEC.md, Social module): presence is OFF by default; when the global
//! toggle is off nothing is published and any existing activity is cleared; games whose capability
//! status is Unsupported / Not investigated never publish.

use crate::capability::Status;
use crate::{Error, Result};
use serde_json::{json, Map, Value};
use std::path::PathBuf;

pub const RPC_VERSION: u32 = 1;
pub const MAX_FRAME: usize = 64 * 1024;
const HEADER: usize = 8;
const MAX_READS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    Handshake,
    Frame,
    Close,
    Ping,
    Pong,
}

impl Opcode {
    pub fn to_u32(self) -> u32 {
        match self {
            Opcode::Handshake => 0,
            Opcode::Frame => 1,
            Opcode::Close => 2,
            Opcode::Ping => 3,
            Opcode::Pong => 4,
        }
    }
    pub fn from_u32(v: u32) -> Option<Self> {
        match v {
            0 => Some(Opcode::Handshake),
            1 => Some(Opcode::Frame),
            2 => Some(Opcode::Close),
            3 => Some(Opcode::Ping),
            4 => Some(Opcode::Pong),
            _ => None,
        }
    }
}

/// Wire format: u32 LE opcode, u32 LE payload length, JSON payload.
pub fn encode_frame(op: Opcode, payload: &[u8]) -> Result<Vec<u8>> {
    if payload.len() + HEADER > MAX_FRAME {
        return Err(Error::Invalid(format!("Discord frame too large: {} bytes", payload.len())));
    }
    let mut out = Vec::with_capacity(HEADER + payload.len());
    out.extend_from_slice(&op.to_u32().to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    Ok(out)
}

/// Ok(None) means more bytes are needed. Otherwise returns (opcode, payload, bytes consumed).
pub fn decode_frame(buf: &[u8]) -> Result<Option<(Opcode, Vec<u8>, usize)>> {
    if buf.len() < HEADER {
        return Ok(None);
    }
    let raw_op = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
    let len = u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]) as usize;
    let op = Opcode::from_u32(raw_op).ok_or_else(|| Error::Invalid(format!("unknown Discord opcode {raw_op}")))?;
    if len + HEADER > MAX_FRAME {
        return Err(Error::Invalid(format!("Discord frame too large: {len} bytes")));
    }
    if buf.len() < HEADER + len {
        return Ok(None);
    }
    Ok(Some((op, buf[HEADER..HEADER + len].to_vec(), HEADER + len)))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Presence {
    pub details: String,
    pub state: String,
    /// Asset key uploaded in the Discord developer portal for this application.
    pub large_image: String,
    pub large_text: String,
    pub start_unix: Option<u64>,
}

/// Discord needs 2..=128 characters. Control characters are stripped. Returns None if unusable.
pub fn sanitize_text(s: &str) -> Option<String> {
    let cleaned: String = s.chars().filter(|c| !c.is_control()).collect();
    let t = cleaned.trim();
    if t.chars().count() < 2 {
        return None;
    }
    Some(t.chars().take(128).collect())
}

pub fn activity_json(p: &Presence) -> Value {
    let mut a = Map::new();
    if let Some(d) = sanitize_text(&p.details) {
        a.insert("details".into(), json!(d));
    }
    if let Some(s) = sanitize_text(&p.state) {
        a.insert("state".into(), json!(s));
    }
    if let Some(t) = p.start_unix {
        a.insert("timestamps".into(), json!({ "start": t }));
    }
    let image = p.large_image.trim();
    if !image.is_empty() {
        let mut assets = Map::new();
        assets.insert("large_image".into(), json!(image));
        if let Some(t) = sanitize_text(&p.large_text) {
            assets.insert("large_text".into(), json!(t));
        }
        a.insert("assets".into(), Value::Object(assets));
    }
    Value::Object(a)
}

/// `activity: None` clears the presence.
pub fn set_activity_payload(pid: u32, activity: Option<&Presence>, nonce: u64) -> Value {
    let mut args = Map::new();
    args.insert("pid".into(), json!(pid));
    if let Some(p) = activity {
        args.insert("activity".into(), activity_json(p));
    }
    json!({ "cmd": "SET_ACTIVITY", "args": Value::Object(args), "nonce": nonce.to_string() })
}

pub trait Transport {
    fn write_frame(&mut self, op: Opcode, payload: &Value) -> Result<()>;
    fn read_frame(&mut self) -> Result<(Opcode, Value)>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublishOutcome {
    Sent,
    DisabledByUser,
    UnsupportedGame,
}

pub struct RichPresence<T: Transport> {
    client_id: String,
    transport: T,
    enabled: bool,
    connected: bool,
    nonce: u64,
}

fn is_error_event(v: &Value) -> bool {
    v.get("evt").and_then(Value::as_str) == Some("ERROR")
}

impl<T: Transport> RichPresence<T> {
    /// Starts disabled (opt-in) and disconnected.
    pub fn new(client_id: &str, transport: T) -> Self {
        Self { client_id: client_id.to_string(), transport, enabled: false, connected: false, nonce: 0 }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn connect(&mut self) -> Result<()> {
        self.transport.write_frame(Opcode::Handshake, &json!({ "v": RPC_VERSION, "client_id": self.client_id }))?;
        for _ in 0..MAX_READS {
            let (op, v) = self.transport.read_frame()?;
            match op {
                Opcode::Ping => self.transport.write_frame(Opcode::Pong, &v)?,
                Opcode::Close => return Err(Error::Invalid(format!("Discord closed the connection: {v}"))),
                Opcode::Frame => {
                    if is_error_event(&v) {
                        return Err(Error::Invalid(format!("Discord rejected the handshake: {}", v["data"])));
                    }
                    if v.get("evt").and_then(Value::as_str) == Some("READY") {
                        self.connected = true;
                        return Ok(());
                    }
                }
                _ => {}
            }
        }
        Err(Error::Invalid("Discord did not answer the handshake with READY".into()))
    }

    fn send_activity(&mut self, activity: Option<&Presence>) -> Result<()> {
        if !self.connected {
            return Err(Error::Invalid("not connected to Discord".into()));
        }
        self.nonce += 1;
        let payload = set_activity_payload(std::process::id(), activity, self.nonce);
        self.transport.write_frame(Opcode::Frame, &payload)?;
        for _ in 0..MAX_READS {
            let (op, v) = self.transport.read_frame()?;
            match op {
                Opcode::Ping => self.transport.write_frame(Opcode::Pong, &v)?,
                Opcode::Close => {
                    self.connected = false;
                    return Err(Error::Invalid(format!("Discord closed the connection: {v}")));
                }
                Opcode::Frame => {
                    if is_error_event(&v) {
                        return Err(Error::Invalid(format!("Discord rejected the activity: {}", v["data"])));
                    }
                    return Ok(());
                }
                _ => {}
            }
        }
        Err(Error::Invalid("Discord did not acknowledge the activity".into()))
    }

    /// The global on/off switch. Turning it off clears any activity Discord is showing.
    pub fn set_enabled(&mut self, on: bool) -> Result<()> {
        let was = self.enabled;
        self.enabled = on;
        if !on && was && self.connected {
            self.send_activity(None)
        } else {
            Ok(())
        }
    }

    /// Publishes only when the global toggle is on AND the game's capability status allows it.
    pub fn publish(&mut self, status: Status, p: &Presence) -> Result<PublishOutcome> {
        if !self.enabled {
            return Ok(PublishOutcome::DisabledByUser);
        }
        if !matches!(status, Status::SupportedTested | Status::Partial | Status::Experimental) {
            return Ok(PublishOutcome::UnsupportedGame);
        }
        self.send_activity(Some(p))?;
        Ok(PublishOutcome::Sent)
    }
}

// ---- real transport: Windows named pipe / Unix socket ----

#[cfg(windows)]
type Io = std::fs::File;
#[cfg(unix)]
type Io = std::os::unix::net::UnixStream;

pub struct IpcTransport {
    io: Io,
}

fn io_err(e: std::io::Error) -> Error {
    Error::Io { path: PathBuf::from("discord-ipc"), source: e }
}

#[cfg(windows)]
fn open_pipe(n: u32) -> Option<Io> {
    std::fs::OpenOptions::new().read(true).write(true).open(format!(r"\\.\pipe\discord-ipc-{n}")).ok()
}

#[cfg(unix)]
fn open_pipe(n: u32) -> Option<Io> {
    let base = std::env::var("XDG_RUNTIME_DIR")
        .or_else(|_| std::env::var("TMPDIR"))
        .or_else(|_| std::env::var("TMP"))
        .or_else(|_| std::env::var("TEMP"))
        .unwrap_or_else(|_| "/tmp".to_string());
    std::os::unix::net::UnixStream::connect(format!("{base}/discord-ipc-{n}")).ok()
}

impl IpcTransport {
    /// Tries discord-ipc-0 .. discord-ipc-9 like the official library.
    pub fn connect() -> Result<Self> {
        for n in 0..10 {
            if let Some(io) = open_pipe(n) {
                return Ok(Self { io });
            }
        }
        Err(Error::Invalid("Discord does not appear to be running (no IPC pipe found)".into()))
    }
}

impl Transport for IpcTransport {
    fn write_frame(&mut self, op: Opcode, payload: &Value) -> Result<()> {
        use std::io::Write;
        let bytes = encode_frame(op, payload.to_string().as_bytes())?;
        self.io.write_all(&bytes).map_err(io_err)?;
        self.io.flush().map_err(io_err)?;
        Ok(())
    }

    fn read_frame(&mut self) -> Result<(Opcode, Value)> {
        use std::io::Read;
        let mut head = [0u8; HEADER];
        self.io.read_exact(&mut head).map_err(io_err)?;
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        if len + HEADER > MAX_FRAME {
            return Err(Error::Invalid(format!("Discord frame too large: {len} bytes")));
        }
        let mut buf = head.to_vec();
        buf.resize(HEADER + len, 0);
        self.io.read_exact(&mut buf[HEADER..]).map_err(io_err)?;
        match decode_frame(&buf)? {
            Some((op, payload, _)) => {
                let v: Value = serde_json::from_slice(&payload)?;
                Ok((op, v))
            }
            None => Err(Error::Invalid("short Discord frame".into())),
        }
    }
}
