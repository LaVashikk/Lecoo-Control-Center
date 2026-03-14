use bincode::{Decode, Encode, config};
use interprocess::local_socket::{GenericNamespaced, Stream, ToNsName};
use std::io::{Read, self, Write};

mod client;
mod server;
mod structs;

pub use client::IpcClient;
pub use server::IpcServer;
pub use structs::*;

#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode)]
pub enum IpcRequest {
    Ping,
    Read(u16),

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
    pub fn send<T: Encode>(&mut self, msg: &T) -> io::Result<()> {
        let data = bincode::encode_to_vec(msg, config::standard())
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        let len = data.len() as u32;

        self.stream.write_all(&len.to_le_bytes())?;
        self.stream.write_all(&data)?;
        self.stream.flush()?;
        Ok(())
    }

    pub fn recv<T: Decode<()>>(&mut self) -> io::Result<T> {
        let mut len_bytes = [0u8; 4];
        self.stream.read_exact(&mut len_bytes)?;
        let len = u32::from_le_bytes(len_bytes) as usize;

        if len > 10 * 1024 * 1024 {
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
