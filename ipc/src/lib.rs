use interprocess::local_socket::{GenericNamespaced, Stream, ToNsName};
use lecoo_types::{caps::*, ec_types::*, settings::CurrentSettings};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    io::{self, Read, Write},
    marker::PhantomData,
};

mod client;
mod frame;
mod server;
pub use client::IpcClient;
pub use server::IpcServer;

pub const IPC_PROTOCOL_MAJOR: u8 = 1;
pub const HANDSHAKE_LEN: usize = 5;
pub const MAGIC_REQ: &[u8; 3] = b"LCC";
pub const MAGIC_OK: &[u8; 3] = b"OKK";
pub const MAGIC_ERR: &[u8; 3] = b"ERR";

// TODO: TOO BIG FILE! Maybe refactor it

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DaemonCommand {
    GetCapabilities,
    RestoreDefaults,
    GetSettings,
    ApplySettings,
    GetTelemetryId,
    ActivateTelemetry(bool),

    /// Raw byte at an absolute EC address
    Read(u16),
    /// Raw write to an absolute EC address.
    /// Only daemons built with `unsafe_write_using_ipc` actually execute it
    Write(u16, u8),

    ActivateProcessSuspend(bool),

    RunPrepareShutdown, // todo: is this actually used?
    RunPrepareSuspend,
    RunPrepareResume,

    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum IpcRequest {
    /// Request the current telemetry and configuration state
    GetSystemState,
    GetFansRPM,
    GetTemperatures,
    GetChargeStatus,
    /// TODO: LEGACY api for reading charge limit
    GetChargeLimit,
    GetPowerProfile,
    GetKeyboardBacklight,

    /// Apply a new power profile (Silent/Default/Performance)
    SetPowerProfile(PowerProfile),

    /// Set a specific fan's mode (Auto/Full/Custom)
    SetFanMode {
        fan: FanIndex,
        mode: FanMode,
    },

    /// Set keyboard backlight brightness
    SetKeyboardBacklight(KeyboardBacklightLevel),

    /// Set battery charge threshold
    SetChargeIntent(ChargeIntent),
    /// TODO: LEGACY api for changing charge
    SetChargeLimit(ChargeLimit),

    /// Control the LED Ring
    SetLedMode(PowerLedMode),

    /// Send a command to the daemon
    DaemonCommand(DaemonCommand),

    #[serde(other)]
    Unknown,
}

/// Responses sent FROM the Daemon TO the Client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "t", content = "c")]
pub enum IpcResponse {
    Success,
    Error(IpcError),

    /// Information about the embedded controller
    SystemInfo(SystemInfo),

    /// RPM readings for both fans
    FanRpm {
        cpu: u16,
        gpu: u16,
    },

    /// Temperature readings for CPU and System
    Temps {
        cpu_c: u8,
        sys_c: u8,
    },

    /// Current battery charge limit (TODO: LEGACY)
    ChargeLimit {
        min: u8,
        max: u8,
        current: u8,
    },

    /// Current battery charge status
    ChargeStatus(ChargeStatus),

    /// Current keyboard backlight brightness
    KeyboardBacklight(KeyboardBacklightLevel),

    /// Current power profile
    PowerLimit(PowerProfile),

    Settings(Box<CurrentSettings>),
    TelemetryId(u64),
    Capabilities(Box<Capabilities>),
    Byte(u8),

    /// Information about telemetry being disabled
    TelemetryDisabledInfo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    Internal,
    /// Daemon does not support this request or the client is too new.
    UnsupportedRequest,
    /// Daemon does not support the hardware the client is using.
    UnsupportedHardware,
    /// Feature exists but preconditions are unmet. Text goes into a GUI tooltip.
    Precondition,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemInfo {
    pub chip: String,
    pub revision: String,
    pub hram_offset: u16,
    pub daemon_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IpcError {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unsupported: Option<UnsupportedInfo>,
}

impl IpcError {
    pub fn new(code: ErrorCode, message: impl Into<String>, unsupported: Option<UnsupportedInfo>) -> Self {
        Self { code, message: message.into(), unsupported }
    }
}

// ---------

pub struct IpcConnection<Tx, Rx> {
    stream: Stream,
    _marker: PhantomData<(Tx, Rx)>,
}

pub type DaemonClient = IpcConnection<IpcRequest, IpcResponse>;
pub type DaemonWorker = IpcConnection<IpcResponse, IpcRequest>;

impl<Tx, Rx> IpcConnection<Tx, Rx>
where
    Tx: Serialize,
    Rx: DeserializeOwned,
{
    pub fn new(stream: Stream) -> Self {
        Self { stream, _marker: PhantomData }
    }

    pub fn send(&mut self, msg: &Tx) -> io::Result<()> {
        frame::write_frame(&mut self.stream, msg)
    }

    pub fn recv(&mut self) -> io::Result<Rx> {
        frame::read_frame(&mut self.stream)
    }

    pub fn connect_client_handshake(stream: Stream) -> io::Result<Self> {
        let mut conn = Self::new(stream);

        // Handshake
        let handshake = [b'L', b'C', b'C', crate::IPC_PROTOCOL_MAJOR, 0];
        conn.stream.write_all(&handshake)?;

        let mut resp = [0u8; HANDSHAKE_LEN];
        conn.stream.read_exact(&mut resp)?;

        if &resp[0..3] == b"ERR" {
            return Err(std::io::Error::new(
                std::io::ErrorKind::ConnectionRefused,
                "Daemon rejected connection: IPC Protocol mismatch! Please update",
            ));
        } else if &resp[0..3] != b"OKK" {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid IPC handshake"));
        }

        Ok(conn)
    }

    pub fn accept_handshake(&mut self) -> io::Result<()> {
        let mut req = [0u8; HANDSHAKE_LEN];
        self.stream.read_exact(&mut req)?;

        if &req[0..3] != MAGIC_REQ {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid magic bytes"));
        }

        if req[3] != IPC_PROTOCOL_MAJOR {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("IPC protocol v{} != v{}", req[3], IPC_PROTOCOL_MAJOR),
            ));
        }

        let resp = [b'O', b'K', b'K', 67, 69];
        self.stream.write_all(&resp)?;

        Ok(())
    }
}

fn get_socket_name() -> io::Result<interprocess::local_socket::Name<'static>> {
    "lecoo_ctl_daemon".to_ns_name::<GenericNamespaced>().map(|n| n.into_owned())
}

const fn parse_u8(s: &str) -> u8 {
    let mut res: u8 = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        res = res * 10 + (bytes[i] - b'0');
        i += 1;
    }
    res
}
