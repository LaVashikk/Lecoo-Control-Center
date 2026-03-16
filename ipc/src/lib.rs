use bincode::{Decode, Encode, config};
use interprocess::local_socket::{GenericNamespaced, Stream, ToNsName};
use std::io::{Read, self, Write};

mod client;
mod server;
mod structs;

pub use client::IpcClient;
pub use server::IpcServer;
pub use structs::*;

pub const IPC_PROTOCOL_VERSION: [u8; 3] = [
    parse_u8(env!("CARGO_PKG_VERSION_MAJOR")),
    parse_u8(env!("CARGO_PKG_VERSION_MINOR")),
    parse_u8(env!("CARGO_PKG_VERSION_PATCH")),
];

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum IpcRequest {
    /// Request the current telemetry and configuration state
    GetSystemState,

    /// Get fans RPM
    GetFansRPM,

    /// Get system temperatures
    GetTemperatures,

    /// Get the current battery charge limit
    GetChargeLimit,

    /// Get the current power profile
    GetPowerProfile,

    /// Get the current keyboard backlight brightness
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
    SetChargeLimit(ChargeLimit),

    /// Control the LED Ring
    SetLedMode(PowerLedMode),
}

/// Responses sent FROM the Daemon TO the Client.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum IpcResponse {
    /// Acknowledgment of a successful command execution
    Success,

    /// Informational message from the daemon
    Message(String),

    /// RPM readings for both fans
    FanRPM(u16, u16),

    /// Temperature readings for CPU and System
    Temp(u8, u8),

    /// Current battery charge limit (min/max percentages)
    ChargeLimit(u8, u8),

    /// Current keyboard backlight brightness
    KeyboardBacklight(u8),

    /// Current power profile
    PowerLimit(PowerProfile),

    /// Error message if something went wrong
    Error(String),
}

pub struct IpcConnection {
    stream: Stream,
}

impl IpcConnection {
    pub fn accept_handshake(&mut self) -> io::Result<()> {
        let mut req = [0u8; 5];
        self.stream.read_exact(&mut req)?;

        if &req[0..3] != b"LCC" {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "Invalid magic bytes"));
        }

        let client_major_ver = req[3];
        let client_minor_ver = req[4];

        if client_major_ver != IPC_PROTOCOL_VERSION[0] || client_minor_ver != IPC_PROTOCOL_VERSION[1] {
            let resp = [b'E', b'R', b'R', IPC_PROTOCOL_VERSION[0], IPC_PROTOCOL_VERSION[1]];
            let _ = self.stream.write_all(&resp);
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Version mismatch. Client: v{}.{}, Server: v{}.{}",
                    client_major_ver, client_minor_ver,
                    IPC_PROTOCOL_VERSION[0], IPC_PROTOCOL_VERSION[1]
                )
            ));
        }

        let resp = [b'O', b'K', b'K', IPC_PROTOCOL_VERSION[0], IPC_PROTOCOL_VERSION[1]];
        self.stream.write_all(&resp)?;

        Ok(())
    }

    pub fn send<T: Encode>(&mut self, msg: &T) -> io::Result<()> {
        let data = bincode::encode_to_vec(msg, config::standard())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let len = data.len() as u32;

        self.stream.write_all(&len.to_le_bytes())?;
        self.stream.write_all(&IPC_PROTOCOL_VERSION)?;
        self.stream.write_all(&data)?;
        self.stream.flush()?;
        Ok(())
    }

    pub fn recv<T: Decode<()>>(&mut self) -> io::Result<T> {
        let mut len_bytes = [0u8; 4];
        let mut bytes_read = 0;

        while bytes_read < 4 {
            let n = self.stream.read(&mut len_bytes[bytes_read..])?;
            if n == 0 {
                if bytes_read == 0 {
                    return Err(io::Error::new(io::ErrorKind::ConnectionReset, "Client disconnected cleanly"));
                } else {
                    return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Connection dropped while reading"));
                }
            }
            bytes_read += n;
        }

        let len = u32::from_le_bytes(len_bytes) as usize;

        let mut msg_version = [0u8; 3];
        self.stream.read_exact(&mut msg_version)?;

        if msg_version[0] != IPC_PROTOCOL_VERSION[0] || msg_version[1] != IPC_PROTOCOL_VERSION[1] {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "IPC protocol version mismatch"));
        }

        if len > 5 * 1024 * 1024 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "IPC payload too large"));
        }

        let mut data = vec![0u8; len];
        self.stream.read_exact(&mut data)?;

        let (msg, _) = bincode::decode_from_slice(&data, config::standard())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        Ok(msg)
    }
}

fn get_socket_name() -> io::Result<interprocess::local_socket::Name<'static>> {
    "lecoo_ctl_daemon"
        .to_ns_name::<GenericNamespaced>()
        .map(|n| n.into_owned())
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
